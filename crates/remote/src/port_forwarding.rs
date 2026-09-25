use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{
        LazyLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use futures::{
    AsyncReadExt as _, AsyncWriteExt as _, FutureExt as _, StreamExt as _,
    channel::mpsc::UnboundedSender,
};
use gpui::{BackgroundExecutor, Task};
use parking_lot::Mutex;
use rpc::{
    AnyProtoClient,
    proto::{self, REMOTE_SERVER_PROJECT_ID},
};

use crate::transport::docker::{DockerConnectionOptions, ForwardNotice, ForwardedPort};

const POLL_INTERVAL: Duration = Duration::from_secs(3);
/// How long to wait after a failed `accept`, e.g. when out of file descriptors.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
/// Consecutive `accept` failures after which a port stops being forwarded, so that
/// it's released and forwarded again once it can be.
const MAX_ACCEPT_FAILURES: usize = 50;

static NEXT_TUNNEL_ID: AtomicU64 = AtomicU64::new(0);

/// The container ports forwarded by some connection, since several windows can
/// be connected to one container.
static FORWARDED_PORTS: LazyLock<Mutex<HashSet<(String, u16)>>> = LazyLock::new(Default::default);

/// Forwards the ports that start listening in a dev container to this machine,
/// like VS Code's automatic port forwarding. The remote server reports the ports,
/// and each connection travels through its existing connection to Zed.
pub(crate) struct PortForwarder {
    pub(crate) client: AnyProtoClient,
    pub(crate) connection_options: DockerConnectionOptions,
    pub(crate) listener: Option<UnboundedSender<PortForwardingEvent>>,
    pub(crate) executor: BackgroundExecutor,
}

/// What happened to the ports of a dev container's connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortForwardingEvent {
    /// A port started being forwarded, or couldn't be.
    Forwarded(ForwardedPort),
    /// A port isn't forwarded anymore.
    Stopped { container_id: String, port: u16 },
}

enum PortCommand {
    Forward(u16),
    Stop(u16),
}

/// The command channels of the running forwarders, by container.
static FORWARDER_COMMANDS: LazyLock<Mutex<HashMap<String, Vec<UnboundedSender<PortCommand>>>>> =
    LazyLock::new(Default::default);

fn send_command(container_id: &str, command: impl Fn() -> PortCommand) {
    let mut forwarders = FORWARDER_COMMANDS.lock();
    if let Some(senders) = forwarders.get_mut(container_id) {
        senders.retain(|sender| sender.unbounded_send(command()).is_ok());
        if senders.is_empty() {
            forwarders.remove(container_id);
        }
    }
}

/// Takes a forwarder's command channel out of [`FORWARDER_COMMANDS`] once the
/// forwarder stops.
struct ForwarderRegistration {
    container_id: String,
    sender: UnboundedSender<PortCommand>,
}

impl ForwarderRegistration {
    fn new(container_id: &str, sender: UnboundedSender<PortCommand>) -> Self {
        FORWARDER_COMMANDS
            .lock()
            .entry(container_id.to_string())
            .or_default()
            .push(sender.clone());
        Self {
            container_id: container_id.to_string(),
            sender,
        }
    }
}

impl Drop for ForwarderRegistration {
    fn drop(&mut self) {
        let mut forwarders = FORWARDER_COMMANDS.lock();
        if let Some(senders) = forwarders.get_mut(&self.container_id) {
            senders.retain(|sender| !sender.same_receiver(&self.sender));
            if senders.is_empty() {
                forwarders.remove(&self.container_id);
            }
        }
    }
}

/// Forwards `port` of the dev container `container_id` to this machine, even if
/// its `portsAttributes` leave it alone.
pub fn forward_container_port(container_id: &str, port: u16) {
    send_command(container_id, || PortCommand::Forward(port));
}

/// Stops forwarding `port` of the dev container `container_id` until it's
/// forwarded again with [`forward_container_port`].
pub fn stop_forwarding_container_port(container_id: &str, port: u16) {
    send_command(container_id, || PortCommand::Stop(port));
}

/// Releases a container port for other connections once its forward stops.
struct ForwardedPortClaim(String, u16);

impl Drop for ForwardedPortClaim {
    fn drop(&mut self) {
        FORWARDED_PORTS.lock().remove(&(self.0.clone(), self.1));
    }
}

impl ForwardedPortClaim {
    fn new(container_id: &str, port: u16) -> Option<Self> {
        FORWARDED_PORTS
            .lock()
            .insert((container_id.to_string(), port))
            .then(|| Self(container_id.to_string(), port))
    }
}

/// The ports a forwarder forwards, which it reports as stopped when they are
/// dropped, e.g. when the window connected to the container closes.
struct Forwards {
    container_id: String,
    listener: Option<UnboundedSender<PortForwardingEvent>>,
    ports: HashMap<u16, (ForwardedPortClaim, Task<()>)>,
}

impl Forwards {
    fn stop(&mut self, port: u16) {
        if self.ports.remove(&port).is_some() {
            self.report_stopped(port);
        }
    }

    fn report_stopped(&self, port: u16) {
        if let Some(listener) = &self.listener {
            listener
                .unbounded_send(PortForwardingEvent::Stopped {
                    container_id: self.container_id.clone(),
                    port,
                })
                .ok();
        }
    }
}

impl Drop for Forwards {
    fn drop(&mut self) {
        for port in self.ports.keys() {
            self.report_stopped(*port);
        }
    }
}

impl PortForwarder {
    pub(crate) async fn run(self) {
        let container_id = self.connection_options.container_id.clone();
        let (command_sender, mut commands) = futures::channel::mpsc::unbounded();
        let _registration = ForwarderRegistration::new(&container_id, command_sender);
        let (failed_sender, mut failed_ports) = futures::channel::mpsc::unbounded();
        let mut handled = HashSet::new();
        let mut forwards = Forwards {
            container_id: container_id.clone(),
            listener: self.listener.clone(),
            ports: HashMap::new(),
        };
        loop {
            match self.listening_ports().await {
                Ok(ports) => {
                    for port in ports {
                        if handled.contains(&port) {
                            continue;
                        }
                        // Another connection to the container forwards it; this one
                        // takes over if that one stops.
                        let Some(claim) = ForwardedPortClaim::new(&container_id, port) else {
                            continue;
                        };
                        handled.insert(port);
                        if let Some(forward) = self.forward(port, false, &failed_sender).await {
                            forwards.ports.insert(port, (claim, forward));
                        }
                    }
                }
                Err(error) => log::debug!("Failed to list the dev container's ports: {error:#}"),
            }

            let mut timer = self.executor.timer(POLL_INTERVAL).fuse();
            futures::select_biased! {
                command = commands.next() => match command {
                    Some(PortCommand::Forward(port)) => {
                        handled.insert(port);
                        if !forwards.ports.contains_key(&port)
                            && let Some(claim) = ForwardedPortClaim::new(&container_id, port)
                            && let Some(forward) = self.forward(port, true, &failed_sender).await
                        {
                            forwards.ports.insert(port, (claim, forward));
                        }
                    }
                    // Stays handled, so it isn't forwarded again automatically.
                    Some(PortCommand::Stop(port)) => {
                        handled.insert(port);
                        forwards.stop(port);
                    }
                    None => {}
                },
                // Forwarded again at the next poll, if it can be by then.
                port = failed_ports.next() => if let Some(port) = port {
                    handled.remove(&port);
                    forwards.stop(port);
                },
                _ = timer => {}
            }
        }
    }

    async fn listening_ports(&self) -> Result<BTreeSet<u16>> {
        let response = self
            .client
            .request(proto::ListListeningPorts {
                project_id: REMOTE_SERVER_PROJECT_ID,
            })
            .await?;
        Ok(response
            .ports
            .into_iter()
            .filter_map(|port| u16::try_from(port).ok())
            .collect())
    }

    /// Starts forwarding `port`, as its `portsAttributes` ask unless `requested`
    /// by the user.
    async fn forward(
        &self,
        port: u16,
        requested: bool,
        failed_sender: &UnboundedSender<u16>,
    ) -> Option<Task<()>> {
        let options = &self.connection_options;
        // Ports the engine publishes already reach this machine.
        if options.forward_ports.contains(&port) && !requested {
            return None;
        }
        let (label, notice) = match options.auto_forward.forwarding(port) {
            Some((label, notice)) => (label, notice),
            None if requested => (None, ForwardNotice::Silent),
            None => return None,
        };
        let listener = bind_local_port(port, options.auto_forward.requires_local_port(port)).await;
        let local_port = listener
            .as_ref()
            .and_then(|listener| listener.local_addr().ok())
            .map(|address| address.port());
        let forward = match (listener, local_port) {
            (Some(listener), Some(local_port)) => {
                log::info!("Forwarding dev container port {port} to localhost:{local_port}");
                Some(self.executor.spawn(accept(
                    listener,
                    port,
                    self.client.clone(),
                    self.executor.clone(),
                    failed_sender.clone(),
                )))
            }
            _ => {
                log::warn!("Not forwarding dev container port {port}: it's in use here");
                None
            }
        };
        if let Some(listener) = &self.listener {
            listener
                .unbounded_send(PortForwardingEvent::Forwarded(ForwardedPort {
                    container_id: options.container_id.clone(),
                    port,
                    local_port,
                    label: label.map(str::to_string),
                    // The user asked for it, so the notification isn't needed.
                    notice: if requested {
                        ForwardNotice::Silent
                    } else {
                        notice
                    },
                    https: options.auto_forward.uses_https(port),
                }))
                .ok();
        }
        forward
    }
}

async fn accept(
    listener: smol::net::TcpListener,
    port: u16,
    client: AnyProtoClient,
    executor: BackgroundExecutor,
    failed_sender: UnboundedSender<u16>,
) {
    let mut failures = 0;
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                failures = 0;
                let client = client.clone();
                executor
                    .spawn(async move {
                        if let Err(error) = tunnel(client, stream, port).await {
                            log::debug!("Port {port} tunnel ended: {error:#}");
                        }
                    })
                    .detach();
            }
            Err(error) => {
                failures += 1;
                if failures >= MAX_ACCEPT_FAILURES {
                    log::warn!("Stopped forwarding dev container port {port}: {error}");
                    failed_sender.unbounded_send(port).ok();
                    return;
                }
                log::debug!("Failed to accept a connection to port {port}: {error}");
                executor.timer(ACCEPT_RETRY_DELAY).await;
            }
        }
    }
}

/// Carries one connection to `port` through the remote server.
async fn tunnel(client: AnyProtoClient, stream: smol::net::TcpStream, port: u16) -> Result<()> {
    let tunnel_id = NEXT_TUNNEL_ID.fetch_add(1, Ordering::Relaxed);
    let mut responses = client
        .request_stream(proto::OpenPortTunnel {
            project_id: REMOTE_SERVER_PROJECT_ID,
            tunnel_id,
            port: u32::from(port),
        })
        .await?;
    let (mut from_local, mut to_local) = stream.split();
    let upload = async {
        let mut buffer = vec![0; 64 * 1024];
        loop {
            match from_local.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(length) => {
                    let data = proto::PortTunnelData {
                        project_id: REMOTE_SERVER_PROJECT_ID,
                        tunnel_id,
                        data: buffer[..length].to_vec(),
                    };
                    if client.send(data).is_err() {
                        break;
                    }
                }
            }
        }
    }
    .fuse();
    let download = async {
        while let Some(Ok(response)) = responses.next().await {
            if to_local.write_all(&response.data).await.is_err() {
                break;
            }
        }
        to_local.close().await.ok();
    }
    .fuse();
    futures::pin_mut!(upload, download);
    let close = proto::ClosePortTunnel {
        project_id: REMOTE_SERVER_PROJECT_ID,
        tunnel_id,
    };
    futures::select_biased! {
        () = download => {
            // The server has dropped its end of the tunnel, so what the connection
            // still sends has nowhere to go.
            client.send(close).ok();
        }
        () = upload => {
            // Lets the port see the end of what was sent, while its answer keeps coming.
            client.send(close).ok();
            download.await;
        }
    }
    Ok(())
}

/// Listens on this machine for connections to forward to `port` of the container:
/// on the same port when it's free, else, unless `require_local_port`, on the next
/// free port after it, like VS Code.
async fn bind_local_port(port: u16, require_local_port: bool) -> Option<smol::net::TcpListener> {
    if let Ok(listener) = smol::net::TcpListener::bind(("127.0.0.1", port)).await {
        return Some(listener);
    }
    if require_local_port {
        return None;
    }
    for candidate in port.saturating_add(1)..=port.saturating_add(100) {
        if let Ok(listener) = smol::net::TcpListener::bind(("127.0.0.1", candidate)).await {
            return Some(listener);
        }
    }
    smol::net::TcpListener::bind(("127.0.0.1", 0)).await.ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn busy_ports_are_forwarded_to_the_next_free_port_unless_required() {
        smol::block_on(async {
            let taken = smol::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let port = taken.local_addr().unwrap().port();

            let fallback = super::bind_local_port(port, false).await.unwrap();
            let fallback_port = fallback.local_addr().unwrap().port();
            assert_ne!(fallback_port, port);

            assert!(super::bind_local_port(port, true).await.is_none());

            drop(taken);
            let same = super::bind_local_port(port, true).await.unwrap();
            assert_eq!(same.local_addr().unwrap().port(), port);
        });
    }

    #[test]
    fn stopped_forwarders_are_forgotten() {
        let (first_sender, _first_commands) = futures::channel::mpsc::unbounded();
        let (second_sender, _second_commands) = futures::channel::mpsc::unbounded();
        let first = super::ForwarderRegistration::new("registered-container", first_sender);
        let second = super::ForwarderRegistration::new("registered-container", second_sender);
        let registered = || {
            super::FORWARDER_COMMANDS
                .lock()
                .get("registered-container")
                .map(Vec::len)
        };
        assert_eq!(registered(), Some(2));
        drop(first);
        assert_eq!(registered(), Some(1));
        drop(second);
        assert_eq!(registered(), None);
    }

    #[test]
    fn a_container_port_is_forwarded_by_one_connection_at_a_time() {
        let claim = super::ForwardedPortClaim::new("claims-container", 3000).unwrap();
        assert!(super::ForwardedPortClaim::new("claims-container", 3000).is_none());
        assert!(super::ForwardedPortClaim::new("claims-container", 3001).is_some());
        assert!(super::ForwardedPortClaim::new("other-container", 3000).is_some());
        drop(claim);
        assert!(super::ForwardedPortClaim::new("claims-container", 3000).is_some());
    }
}
