pub mod engine_host;
pub mod json_log;
mod port_forwarding;
pub use port_forwarding::{
    PortForwardingEvent, forward_container_port, stop_forwarding_container_port,
};
pub mod protocol;
pub mod proxy;
pub mod remote_client;
pub mod remote_identity;
mod transport;

pub use engine_host::{EngineHost, HostCommand, SshEngineHost};
#[cfg(target_os = "windows")]
pub use remote_client::OpenWslPath;
pub use remote_client::{
    CommandTemplate, ConnectionIdentifier, ConnectionState, Interactive, RemoteArch, RemoteClient,
    RemoteClientDelegate, RemoteClientEvent, RemoteConnection, RemoteConnectionOptions, RemoteOs,
    RemotePlatform, connect, has_active_connection,
};
pub use remote_identity::{
    DockerIdentityKey, RemoteConnectionIdentity, remote_connection_identity,
    same_remote_connection_identity,
};
pub use transport::docker::{
    AutoForwardPorts, AutoForwardRule, CONTAINER_SSH_AGENT_SOCKET, DevContainerSecretsFile,
    DockerConnectionOptions, ForwardNotice, ForwardedPort, ForwardedPortListener,
    SERVER_CACHE_PATH, SERVER_CACHE_VOLUME, ShutdownAction, container_cli, is_wslc,
    load_dev_container_secrets, push_secrets, set_use_wslc,
};
pub use transport::ssh::{SshConnectionOptions, SshPortForwardOption};
pub use transport::wsl::WslConnectionOptions;
#[cfg(target_os = "windows")]
pub use transport::wsl::wsl_path_to_windows_path;

#[cfg(any(test, feature = "test-support"))]
pub use transport::mock::{
    MockConnection, MockConnectionOptions, MockConnectionRegistry, MockDelegate,
};
