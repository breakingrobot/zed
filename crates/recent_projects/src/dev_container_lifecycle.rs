use project::trusted_worktrees::TrustedWorktrees;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Weak};

use anyhow::Context as _;
use dev_container::{
    BuildMode, DeferredHook, DevContainerConfig, DevContainerContext, StartedDevContainer,
    find_devcontainer_configs,
};
use futures::StreamExt as _;
use gpui::{
    App, AppContext as _, AsyncApp, AsyncWindowContext, Context, TaskExt as _, WeakEntity, Window,
    WindowHandle, WindowId,
};
use project::TaskSourceKind;
use remote::{
    DockerConnectionOptions, ForwardNotice, ForwardedPort, ForwardedPortListener,
    PortForwardingEvent, RemoteConnectionOptions, ShutdownAction,
};
use task::{TaskContext, TaskTemplate};
use workspace::notifications::{NotificationId, simple_message_notification::MessageNotification};
use workspace::{AppState, MultiWorkspace, OpenOptions, Workspace, tasks::ScheduledTaskResult};

use crate::remote_connections::{
    Connection, dismiss_connection_modal, open_remote_project, set_connection_modal_status,
};

/// Surfaces a lifecycle failure to the user. All of the operations here report
/// errors the same way: a critical modal titled with the operation that
/// failed, detailing the underlying error.
async fn prompt_error(cx: &mut AsyncWindowContext, title: &str, detail: impl std::fmt::Display) {
    cx.prompt(
        gpui::PromptLevel::Critical,
        title,
        Some(&detail.to_string()),
        &["OK"],
    )
    .await
    .ok();
}

/// Surfaces a failure to create or start a dev container, offering the log of
/// what ran, at `log_path`.
pub(crate) async fn prompt_start_error(
    cx: &mut AsyncWindowContext,
    title: &str,
    detail: impl std::fmt::Display,
    log_path: Option<PathBuf>,
) {
    let answer = cx
        .prompt(
            gpui::PromptLevel::Critical,
            title,
            Some(&detail.to_string()),
            &["OK", "Show Log"],
        )
        .await;
    if matches!(answer, Ok(1)) {
        cx.update(|window, cx| {
            if let Some(multi_workspace) = window.root::<MultiWorkspace>().flatten() {
                let workspace = multi_workspace.read(cx).workspace().clone();
                workspace.update(cx, |workspace, cx| {
                    open_dev_container_log(workspace, log_path, window, cx)
                });
            }
        })
        .ok();
    }
}

/// Opens what starting the dev container that `workspace` is connected to ran and
/// printed, or the one started last, followed by what the processes of the dev
/// container printed.
pub(crate) fn show_dev_container_log(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    open_dev_container_log(workspace, None, window, cx);
}

/// Opens the dev container log at `log_path`, or else the one of the dev container
/// `workspace` is connected to, or the one started last.
fn open_dev_container_log(
    workspace: &mut Workspace,
    log_path: Option<PathBuf>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let connection = match workspace.project().read(cx).remote_connection_options(cx) {
        Some(RemoteConnectionOptions::Docker(options)) => Some(options),
        _ => None,
    };
    let log_path = log_path
        .or_else(|| {
            connection
                .as_ref()
                .and_then(|options| options.dev_container_labels())
                .map(|(local_folder, config_file)| {
                    dev_container::dev_container_log_path(local_folder, config_file)
                })
        })
        .or_else(dev_container::last_start_log_path);
    let project = workspace.project().clone();
    let languages = workspace.app_state().languages.clone();
    let fs = workspace.app_state().fs.clone();
    cx.spawn_in(window, async move |workspace, cx| {
        let mut text = match &log_path {
            Some(log_path) => match fs.load(log_path).await {
                Ok(log) if !log.is_empty() => log,
                Ok(_) | Err(_) => format!(
                    "This dev container wasn't started since Zed started ({}).\n",
                    log_path.display()
                ),
            },
            None => "No dev container was started yet.\n".to_string(),
        };
        if let Some(options) = connection {
            let container_log = dev_container::container_logs(
                &options.container_id,
                options.use_podman,
                &options.host,
            )
            .await
            .unwrap_or_else(|error| format!("Couldn't read the container's log: {error}\n"));
            text.push_str(&format!("\n# Container {}\n{container_log}", options.name));
        }
        let language = languages.language_for_name("log").await.ok();
        let buffer = project
            .update(cx, |project, cx| project.create_buffer(language, false, cx))
            .await?;
        buffer.update(cx, |buffer, cx| {
            buffer.set_text(text, cx);
            buffer.set_capability(language::Capability::ReadOnly, cx);
        });
        let buffer = cx.new(|cx| {
            editor::MultiBuffer::singleton(buffer, cx).with_title("Dev Container Log".into())
        });
        workspace.update_in(cx, |workspace, window, cx| {
            let editor = cx.new(|cx| {
                let mut editor =
                    editor::Editor::for_multibuffer(buffer, Some(project.clone()), window, cx);
                editor.set_read_only(true);
                editor
            });
            workspace.add_item_to_active_pane(Box::new(editor), None, true, window, cx);
        })?;
        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}

/// Cleanly tears down the remote connection currently backing `workspace`,
/// if any, *before* we destroy the dev container on the Docker side.
///
/// Without this, the still-live `RemoteClient` would notice its container
/// disappearing out from under it (its next heartbeat/exec would fail), and
/// its own reconnection logic would kick in and race with us reopening the
/// project - repeatedly retrying against a container id that no longer
/// exists, and eventually surfacing a "Disconnected" prompt. Calling
/// `shutdown_processes` bypasses that reconnection logic entirely (unlike
/// `RemoteClient::force_disconnect`, whose docs note it triggers
/// reconnection).
async fn shutdown_remote_connection(
    workspace_handle: &gpui::WeakEntity<Workspace>,
    cx: &mut AsyncWindowContext,
) {
    let shutdown_task = workspace_handle.update(cx, |workspace, cx| {
        workspace
            .project()
            .read(cx)
            .remote_client()
            .and_then(|client| {
                client.update(cx, |client, cx| {
                    client.shutdown_processes(
                        Some(rpc::proto::ShutdownRemoteServer {}),
                        cx.background_executor().clone(),
                    )
                })
            })
    });

    if let Ok(Some(shutdown_task)) = shutdown_task {
        shutdown_task.await;
    }
}

/// Stops the dev container backing the current project and reopens its
/// local folder in the same window. The container itself is left in place
/// (stopped, not removed), so it can be resumed later by reopening it in a
/// dev container again.
pub(crate) fn stop_dev_container(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(RemoteConnectionOptions::Docker(options)) =
        workspace.project().read(cx).remote_connection_options(cx)
    else {
        cx.propagate();
        return;
    };

    let app_state = Arc::downgrade(workspace.app_state());
    let replace_window = window.window_handle().downcast::<MultiWorkspace>();
    let workspace_handle = cx.entity().downgrade();

    cx.spawn_in(window, async move |_, cx| {
        let origin = match dev_container::dev_container_origin(
            &options.container_id,
            options.use_podman,
            &options.host,
        )
        .await
        {
            Ok(origin) => origin,
            Err(e) => {
                log::error!("Failed to determine dev container's local folder: {e}");
                prompt_error(cx, "Failed to stop Dev Container", &e).await;
                return;
            }
        };

        shutdown_remote_connection(&workspace_handle, cx).await;

        if let Err(e) = dev_container::stop_dev_container(
            &options.container_id,
            options.use_podman,
            &options.host,
        )
        .await
        {
            log::error!("Failed to stop dev container: {e}");
            prompt_error(cx, "Failed to stop Dev Container", &e).await;
            return;
        }

        let Some(app_state) = app_state.upgrade() else {
            return;
        };

        let open_task = cx.update(|_, cx| {
            workspace::open_paths(
                &[origin.local_folder],
                app_state,
                OpenOptions {
                    requesting_window: replace_window,
                    ..Default::default()
                },
                cx,
            )
        });

        match open_task {
            Ok(task) => {
                if let Err(e) = task.await {
                    log::error!(
                        "Failed to reopen project locally after stopping dev container: {e:#}"
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to reopen project locally after stopping dev container: {e:#}");
            }
        }
    })
    .detach();
}

/// Stops and removes the dev container backing the current project (a
/// `docker rm -f`), then reopens its local folder in the same window. Unlike
/// [`stop_dev_container`], the container is destroyed - its writable layer and
/// any state not on a mounted volume are lost - so this lets users reclaim
/// resources without dropping to the CLI, at the cost of needing a rebuild to
/// use the container again.
///
/// Because the deletion is irreversible it is confirmed with the user first.
/// Like every lifecycle side effect, this only ever runs on an explicit user
/// action.
pub(crate) fn delete_dev_container(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(RemoteConnectionOptions::Docker(options)) =
        workspace.project().read(cx).remote_connection_options(cx)
    else {
        cx.propagate();
        return;
    };

    let app_state = Arc::downgrade(workspace.app_state());
    let replace_window = window.window_handle().downcast::<MultiWorkspace>();
    let workspace_handle = cx.entity().downgrade();

    cx.spawn_in(window, async move |_, cx| {
        let reopen = replace_window.map(|window| (window, app_state));
        delete_dev_container_with_options(options, vec![workspace_handle], reopen, cx).await;
    })
    .detach();
}

/// Shared implementation of the "Delete Dev Container" action, callable both
/// from a connected workspace (title bar / command palette) and from the
/// thread sidebar's project-group menu (which may target a *stopped* container
/// that has no live workspace).
///
/// - `connected_workspaces`: any live workspaces currently backed by this
///   container; each has its remote connection cleanly shut down before the
///   container is destroyed. Empty for a stopped container.
/// - `reopen`: when `Some`, the container's local folder is reopened in the
///   given window after removal. Callers pass `None` when the container is not
///   backing the active window, so a background/stopped group is never allowed
///   to hijack the user's current window.
pub async fn delete_dev_container_with_options(
    options: DockerConnectionOptions,
    connected_workspaces: Vec<WeakEntity<Workspace>>,
    reopen: Option<(WindowHandle<MultiWorkspace>, Weak<AppState>)>,
    cx: &mut AsyncWindowContext,
) {
    let confirmed = cx
        .prompt(
            gpui::PromptLevel::Warning,
            "Delete this Dev Container?",
            Some(
                "The container will be stopped and removed. Any changes not on a \
                 mounted volume will be lost, and reconnecting later will require \
                 rebuilding it. Your project files on this machine are not affected.",
            ),
            &["Delete", "Cancel"],
        )
        .await;
    if !matches!(confirmed, Ok(0)) {
        return;
    }

    // Only resolve the local folder when we intend to reopen it. Read it from
    // the container's labels *before* removing it; afterwards it can no longer
    // be inspected.
    let local_folder = if reopen.is_some() {
        match dev_container::dev_container_origin(
            &options.container_id,
            options.use_podman,
            &options.host,
        )
        .await
        {
            Ok(origin) => Some(origin.local_folder),
            Err(e) => {
                log::error!("Failed to determine dev container's local folder: {e}");
                prompt_error(cx, "Failed to delete Dev Container", &e).await;
                return;
            }
        }
    } else {
        None
    };

    for workspace_handle in &connected_workspaces {
        shutdown_remote_connection(workspace_handle, cx).await;
    }

    // If the window closed meanwhile, the container's cached environment merely
    // stays until the session ends.
    let session_cache = cx
        .update(|_, cx| dev_container::SessionCache::global(cx))
        .unwrap_or_default();
    if let Err(e) = dev_container::remove_dev_container(
        &options.container_id,
        options.use_podman,
        &options.host,
        &session_cache,
    )
    .await
    {
        log::error!("Failed to remove dev container: {e}");
        prompt_error(cx, "Failed to delete Dev Container", &e).await;
        return;
    }

    let (Some(local_folder), Some((replace_window, app_state))) = (local_folder, reopen) else {
        return;
    };
    let Some(app_state) = app_state.upgrade() else {
        return;
    };

    let open_task = cx.update(|_, cx| {
        workspace::open_paths(
            &[local_folder],
            app_state,
            OpenOptions {
                requesting_window: Some(replace_window),
                ..Default::default()
            },
            cx,
        )
    });

    match open_task {
        Ok(task) => {
            if let Err(e) = task.await {
                log::error!("Failed to reopen project locally after deleting dev container: {e:#}");
            }
        }
        Err(e) => {
            log::error!("Failed to reopen project locally after deleting dev container: {e:#}");
        }
    }
}

/// Removes the current dev container, builds it again from scratch, and
/// reconnects to it in the same window. When invoked while the project is
/// local (no dev container currently connected), this instead opens the
/// dev container creation flow for this project in "rebuild" mode: any
/// existing container matching the chosen project/config is torn down and
/// rebuilt from scratch, then connected to, rather than resumed.
pub(crate) fn rebuild_dev_container(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    rebuild_dev_container_in_mode(workspace, BuildMode::Rebuild, window, cx);
}

/// Like [`rebuild_dev_container`], without the container engine's build cache.
pub(crate) fn rebuild_dev_container_without_cache(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    rebuild_dev_container_in_mode(workspace, BuildMode::RebuildWithoutCache, window, cx);
}

fn rebuild_dev_container_in_mode(
    workspace: &mut Workspace,
    build_mode: BuildMode,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(RemoteConnectionOptions::Docker(options)) =
        workspace.project().read(cx).remote_connection_options(cx)
    else {
        // Not connected to a dev container: fall back to the creation flow in
        // "rebuild" mode, so the chosen project/config is rebuilt from scratch
        // rather than resumed.
        open_dev_container_modal(workspace, build_mode, window, cx);
        return;
    };

    let mode = if build_mode == BuildMode::RebuildWithoutCache {
        ReconnectMode::RebuildWithoutCache
    } else {
        ReconnectMode::Rebuild
    };
    reconnect_connected_dev_container(workspace, options, mode, window, cx);
}

/// Reconnects to the dev container backing the current project, starting the
/// container again if it has stopped. Unlike [`rebuild_dev_container`], the
/// existing container is *resumed* (`docker start` + reconnect) rather than
/// torn down and rebuilt from scratch, so its state is preserved.
///
/// This is the recovery path for a dev container whose connection was lost
/// (e.g. the container exited or was stopped out from under Zed): the raw
/// remote reconnect does not start a stopped container, so a plain
/// "Reconnect" would keep failing against a container that is not running.
pub(crate) fn reconnect_dev_container(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(RemoteConnectionOptions::Docker(options)) =
        workspace.project().read(cx).remote_connection_options(cx)
    else {
        cx.propagate();
        return;
    };

    reconnect_connected_dev_container(workspace, options, ReconnectMode::Resume, window, cx);
}

/// Restarts the dev container backing the current project (stopping then
/// starting it, which kills every in-container process including any wedged
/// `zed-remote-server`) and reconnects. This is the escalation from a plain
/// [`reconnect_dev_container`] for the case where the container is still
/// running but its server is wedged, so resuming would keep reusing the broken
/// server. The container's filesystem is preserved; only its processes die.
///
/// Like every other lifecycle side effect, this must only run in response to an
/// explicit user action.
pub(crate) fn restart_dev_container_and_reconnect(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(RemoteConnectionOptions::Docker(options)) =
        workspace.project().read(cx).remote_connection_options(cx)
    else {
        cx.propagate();
        return;
    };

    reconnect_connected_dev_container(workspace, options, ReconnectMode::Restart, window, cx);
}

/// Opens the dev container config-discovery/selection modal for the current
/// (local) project, shared by the `OpenDevContainer` and `RebuildDevContainer`
/// entry points. Unless `build_mode` reuses containers, any existing container
/// matching the chosen project/config is removed and rebuilt rather than resumed
/// (see `RemoteServerProjects::build_mode`).
pub(crate) fn open_dev_container_modal(
    workspace: &mut Workspace,
    build_mode: BuildMode,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(reason) = dev_container::unsupported_reason(workspace.project().read(cx), cx) {
        let verb = if build_mode.rebuilds() {
            "rebuild"
        } else {
            "open"
        };
        let message = format!("Cannot {verb} Dev Container");
        cx.spawn_in(window, async move |_, cx| {
            cx.prompt(gpui::PromptLevel::Critical, &message, Some(reason), &["OK"])
                .await
                .ok();
        })
        .detach();
        return;
    }

    // Opening a dev container runs code from the repository (`initializeCommand`,
    // Dockerfiles, features), so it waits until the project is trusted.
    if TrustedWorktrees::has_restricted_worktrees(
        &workspace.project().read(cx).worktree_store(),
        cx,
    ) {
        workspace.show_worktree_trust_security_modal(false, window, cx);
        return;
    }

    let fs = workspace.project().read(cx).fs().clone();
    let configs = find_devcontainer_configs(workspace, cx);
    let app_state = workspace.app_state().clone();
    let dev_container_context = DevContainerContext::from_workspace(workspace, cx);
    let handle = cx.entity().downgrade();
    workspace.toggle_modal(window, cx, |window, cx| {
        crate::RemoteServerProjects::new_dev_container(
            fs,
            configs,
            app_state,
            dev_container_context,
            build_mode,
            window,
            handle,
            cx,
        )
    });
}

/// How the dev container currently connected to a workspace is (re)established
/// when recovering it in place.
#[derive(Clone, Copy)]
enum ReconnectMode {
    /// Resume the existing container (`docker start` if it has stopped) and
    /// reconnect. Preserves all in-container state.
    Resume,
    /// Stop then start the container, killing every in-container process
    /// (including a wedged `zed-remote-server`) before reconnecting. The
    /// container and its filesystem are kept.
    Restart,
    /// Remove and rebuild the container from scratch before reconnecting.
    Rebuild,
    /// Like `Rebuild`, without the container engine's build cache.
    RebuildWithoutCache,
}

impl ReconnectMode {
    /// Title shown on the error modal when this flow fails.
    fn error_title(self) -> &'static str {
        match self {
            ReconnectMode::Resume => "Failed to reconnect to Dev Container",
            ReconnectMode::Restart => "Failed to restart Dev Container",
            ReconnectMode::Rebuild | ReconnectMode::RebuildWithoutCache => {
                "Failed to rebuild Dev Container"
            }
        }
    }

    /// Status shown in the connection modal while the container-lifecycle phase
    /// runs, so a slow `docker` step (especially a rebuild, which can take
    /// minutes) doesn't leave the window looking frozen before reconnecting.
    fn status(self) -> &'static str {
        match self {
            ReconnectMode::Resume => "Starting dev container\u{2026}",
            ReconnectMode::Restart => "Restarting dev container\u{2026}",
            ReconnectMode::Rebuild | ReconnectMode::RebuildWithoutCache => {
                "Rebuilding dev container\u{2026}"
            }
        }
    }

    fn build_mode(self) -> BuildMode {
        match self {
            ReconnectMode::Resume | ReconnectMode::Restart => BuildMode::Reuse,
            ReconnectMode::Rebuild => BuildMode::Rebuild,
            ReconnectMode::RebuildWithoutCache => BuildMode::RebuildWithoutCache,
        }
    }
}

/// Shared implementation for resuming, restarting, or rebuilding the dev
/// container currently connected to `workspace` (see [`ReconnectMode`]). In
/// every case we recover the container's origin (host folder + config) from its
/// identifying labels, tear down the live remote connection cleanly, bring the
/// container to a clean running state per `mode`, and reopen the project against
/// the resulting connection in the same window.
fn reconnect_connected_dev_container(
    workspace: &mut Workspace,
    options: DockerConnectionOptions,
    mode: ReconnectMode,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let app_state = Arc::downgrade(workspace.app_state());
    let replace_window = window.window_handle().downcast::<MultiWorkspace>();
    let workspace_handle = cx.entity().downgrade();

    let error_title = mode.error_title();
    let build_mode = mode.build_mode();

    cx.spawn_in(window, async move |_, cx| {
        let origin = match dev_container::dev_container_origin(
            &options.container_id,
            options.use_podman,
            &options.host,
        )
        .await
        {
            Ok(origin) => origin,
            Err(e) => {
                log::error!("Failed to determine dev container's local folder: {e}");
                prompt_error(cx, error_title, &e).await;
                return;
            }
        };

        let context = match workspace_handle.update(cx, |workspace, cx| {
            DevContainerContext::for_local_directory(
                Arc::from(origin.local_folder.as_path()),
                options.host.clone(),
                workspace,
                cx,
            )
        }) {
            Ok(context) => context,
            Err(e) => {
                log::error!("Workspace no longer available to reconnect dev container: {e:#}");
                return;
            }
        };

        let environment = context.environment(cx).await;

        // Surface progress before the potentially slow container-lifecycle work
        // (shutdown/stop/build). The connection modal we open here is a
        // `RemoteConnectionModal`, the same type `open_remote_project` uses for
        // the subsequent connect phase; since it stays open (it only dismisses
        // once marked finished), `open_remote_project` reuses it rather than
        // flashing a new one, so the spinner transitions straight from the
        // container build into connecting.
        show_lifecycle_status(&workspace_handle, &options, mode.status(), cx);

        shutdown_remote_connection(&workspace_handle, cx).await;

        // For a restart, stop the container first so the subsequent start
        // (below) brings up a container with no leftover processes - notably no
        // wedged `zed-remote-server` reusing a still-valid pid file. Rebuild
        // handles teardown itself, and resume intentionally leaves a running
        // container running.
        if matches!(mode, ReconnectMode::Restart) {
            if let Err(e) = dev_container::stop_dev_container(
                &options.container_id,
                options.use_podman,
                &options.host,
            )
            .await
            {
                log::error!("Failed to stop dev container before restart: {e}");
                dismiss_lifecycle_status(&workspace_handle, cx);
                prompt_error(cx, error_title, &e).await;
                return;
            }
        }

        let log_path = dev_container::start_log_path(&context, &origin.config);
        let start_result = dev_container::start_dev_container_with_config(
            context,
            Some(origin.config),
            environment,
            build_mode,
            true,
        )
        .await;

        let StartedDevContainer {
            connection,
            remote_workspace_folder: starting_dir,
            deferred_hooks,
            config_changed,
            warnings,
        } = match start_result {
            Ok(result) => result,
            Err(e) => {
                log::error!("Failed to start dev container: {e}");
                dismiss_lifecycle_status(&workspace_handle, cx);
                prompt_start_error(cx, error_title, &e, Some(log_path)).await;
                return;
            }
        };

        let Some(app_state) = app_state.upgrade() else {
            return;
        };

        let result = open_remote_project(
            Connection::DevContainer(connection).into(),
            vec![PathBuf::from(&starting_dir)],
            app_state,
            OpenOptions {
                requesting_window: replace_window,
                ..OpenOptions::default()
            },
            cx,
        )
        .await;

        match result {
            Ok(window) => {
                show_warnings(window, warnings, cx);
                if config_changed {
                    suggest_rebuild(window, cx);
                }
                run_deferred_hooks(window, starting_dir, deferred_hooks, cx);
            }
            Err(e) => {
                log::error!("Failed to reconnect to dev container: {e:#}");
                prompt_error(cx, "Failed to reconnect", format!("{e:#}")).await;
            }
        }
    })
    .detach();
}

/// Tells the user that the dev container was created from an older configuration,
/// and offers to rebuild it, as VS Code does.
pub(crate) fn suggest_rebuild(window: WindowHandle<MultiWorkspace>, cx: &mut AsyncApp) {
    struct DevContainerConfigChanged;

    window
        .update(cx, |multi_workspace, _window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.show_notification(
                    NotificationId::unique::<DevContainerConfigChanged>(),
                    cx,
                    |cx| {
                        cx.new(|cx| {
                            MessageNotification::new(
                                "The Dev Container configuration changed since the container was created. Rebuild it to apply the changes.",
                                cx,
                            )
                            .primary_message("Rebuild Container")
                            .primary_on_click(|window, cx| {
                                window.dispatch_action(Box::new(zed_actions::RebuildDevContainer), cx);
                            })
                        })
                    },
                );
            })
        })
        .ok();
}

/// Carries out the `shutdownAction` of a dev container once the last window
/// connected to it closes, or when Zed quits, like VS Code.
pub(crate) fn shut_down_dev_containers_when_closed(cx: &mut App) {
    let connections_by_window: Rc<RefCell<HashMap<WindowId, Vec<DockerConnectionOptions>>>> =
        Rc::default();

    cx.observe_new({
        let connections_by_window = connections_by_window.clone();
        move |workspace: &mut Workspace,
              window: Option<&mut Window>,
              cx: &mut Context<Workspace>| {
            let Some(window) = window else {
                return;
            };
            if let Some(RemoteConnectionOptions::Docker(options)) =
                workspace.project().read(cx).remote_connection_options(cx)
                && options.shutdown_action != ShutdownAction::None
            {
                connections_by_window
                    .borrow_mut()
                    .entry(window.window_handle().window_id())
                    .or_default()
                    .push(options);
            }
        }
    })
    .detach();

    cx.on_window_closed({
        let connections_by_window = connections_by_window.clone();
        move |cx, window_id| {
            let Some(connections) = connections_by_window.borrow_mut().remove(&window_id) else {
                return;
            };
            let mut connected = connected_container_ids(cx);
            for options in connections {
                // Also skips a container listed twice for the closed window.
                if !connected.insert(options.container_id.clone()) {
                    continue;
                }
                cx.background_spawn(async move {
                    if let Err(error) = dev_container::shut_down_dev_container(&options).await {
                        log::error!("Failed to shut down dev container: {error}");
                    }
                })
                .detach();
            }
        }
    })
    .detach();

    cx.on_app_quit(move |cx| {
        let connected = connected_container_ids(cx);
        let mut shut_down = HashSet::new();
        for options in connections_by_window
            .borrow_mut()
            .drain()
            .flat_map(|(_, list)| list)
        {
            if !connected.contains(&options.container_id)
                || !shut_down.insert(options.container_id.clone())
            {
                continue;
            }
            // Zed exits before a stop could finish, so the engine is left to do it.
            // The engine environment of a WSL or SSH host would take too long to
            // load, so its defaults are used.
            let Some(command) = dev_container::shutdown_command(&options, &[]) else {
                continue;
            };
            if let Err(error) = command.to_command().spawn() {
                log::error!("Failed to shut down dev container: {error}");
            }
        }
        async {}
    })
    .detach();
}

/// The dev containers that a window is connected to.
fn connected_container_ids(cx: &App) -> HashSet<String> {
    cx.windows()
        .into_iter()
        .filter_map(|window| window.downcast::<MultiWorkspace>())
        .filter_map(|window| window.read(cx).ok())
        .flat_map(|multi_workspace| {
            multi_workspace
                .workspaces()
                .filter_map(|workspace| {
                    match workspace
                        .read(cx)
                        .project()
                        .read(cx)
                        .remote_connection_options(cx)
                    {
                        Some(RemoteConnectionOptions::Docker(options)) => {
                            Some(options.container_id)
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Tells the user about the ports that dev container connections start
/// forwarding, as their `onAutoForward` asks.
pub(crate) fn announce_forwarded_ports(cx: &mut App) {
    let (sender, mut receiver) = futures::channel::mpsc::unbounded();
    cx.set_global(ForwardedPortListener(sender));
    cx.spawn(async move |cx| {
        let mut opened_in_browser = std::collections::HashSet::new();
        while let Some(event) = receiver.next().await {
            let forwarded = match event {
                PortForwardingEvent::Forwarded(forwarded) => forwarded,
                PortForwardingEvent::Stopped { container_id, port } => {
                    cx.update(|cx| crate::forwarded_ports::remove(&container_id, port, cx));
                    continue;
                }
            };
            let Some(local_port) = forwarded.local_port else {
                cx.update(|cx| show_forwarded_port(forwarded, None, cx));
                continue;
            };
            cx.update(|cx| crate::forwarded_ports::insert(forwarded.clone(), cx));
            let url = crate::forwarded_ports::url(&forwarded, local_port);
            match forwarded.notice {
                ForwardNotice::Silent => {}
                ForwardNotice::OpenBrowser => cx.update(|cx| cx.open_url(&url)),
                ForwardNotice::OpenBrowserOnce => {
                    if opened_in_browser.insert((forwarded.container_id.clone(), forwarded.port)) {
                        cx.update(|cx| cx.open_url(&url));
                    }
                }
                ForwardNotice::Notify => {
                    cx.update(|cx| show_forwarded_port(forwarded, Some(url), cx))
                }
            }
        }
    })
    .detach();
}

/// Shows the notification in the window connected to the port's container, with
/// the `url` that reaches the port, or `None` when it couldn't be forwarded.
fn show_forwarded_port(forwarded: ForwardedPort, url: Option<String>, cx: &mut App) {
    struct ForwardedPortNotification;

    let workspace = cx.windows().into_iter().find_map(|window| {
        let window = window.downcast::<MultiWorkspace>()?;
        let multi_workspace = window.read(cx).ok()?;
        let workspace = multi_workspace.workspaces().find(|workspace| {
            matches!(
                workspace.read(cx).project().read(cx).remote_connection_options(cx),
                Some(RemoteConnectionOptions::Docker(options))
                    if options.container_id == forwarded.container_id
            )
        })?;
        Some(workspace.clone())
    });
    let Some(workspace) = workspace else {
        return;
    };
    let port = forwarded.port;
    let name = match &forwarded.label {
        Some(label) => format!("Port {port} ({label})"),
        None => format!("Port {port}"),
    };
    let message = match forwarded.local_port {
        Some(local_port) => format!("{name} is forwarded to localhost:{local_port}."),
        None => format!(
            "{name} isn't forwarded: port {port} is in use on this machine, and its \
             requireLocalPort forbids using another one."
        ),
    };
    workspace.update(cx, |workspace, cx| {
        workspace.show_notification(
            NotificationId::composite::<ForwardedPortNotification>(format!(
                "{}:{port}",
                forwarded.container_id
            )),
            cx,
            |cx| {
                cx.new(|cx| {
                    let notification = MessageNotification::new(message, cx);
                    match url {
                        Some(url) => notification
                            .primary_message("Open in Browser")
                            .primary_on_click(move |_window, cx| cx.open_url(&url)),
                        None => notification,
                    }
                })
            },
        );
    });
}

/// Starts the dev container of `context`'s project from `config`, reusing its
/// container, and opens it in a new window, reporting failures under
/// `error_title`.
pub(crate) async fn start_and_open_dev_container(
    context: DevContainerContext,
    config: DevContainerConfig,
    app_state: Arc<AppState>,
    error_title: &str,
    cx: &mut AsyncWindowContext,
) {
    let environment = context.environment(cx).await;
    let log_path = dev_container::start_log_path(&context, &config);
    let started = dev_container::start_dev_container_with_config(
        context,
        Some(config),
        environment,
        BuildMode::Reuse,
        true,
    )
    .await;
    let StartedDevContainer {
        connection,
        remote_workspace_folder,
        deferred_hooks,
        config_changed,
        warnings,
    } = match started {
        Ok(started) => started,
        Err(error) => {
            log::error!("Failed to start dev container: {error}");
            prompt_start_error(cx, error_title, &error, Some(log_path)).await;
            return;
        }
    };
    let opened = open_remote_project(
        Connection::DevContainer(connection).into(),
        vec![PathBuf::from(&remote_workspace_folder)],
        app_state,
        OpenOptions::default(),
        cx,
    )
    .await;
    match opened {
        Ok(window) => {
            show_warnings(window, warnings, cx);
            if config_changed {
                suggest_rebuild(window, cx);
            }
            run_deferred_hooks(window, remote_workspace_folder, deferred_hooks, cx);
        }
        Err(error) => {
            log::error!("Failed to connect: {error:#}");
            prompt_error(cx, "Failed to connect", format!("{error:#}")).await;
        }
    }
}

/// Shows the problems found while starting the dev container that didn't stop it.
pub(crate) fn show_warnings(
    window: WindowHandle<MultiWorkspace>,
    warnings: Vec<String>,
    cx: &mut AsyncApp,
) {
    struct DevContainerWarning;

    for warning in warnings {
        window
            .update(cx, |multi_workspace, _window, cx| {
                multi_workspace.workspace().update(cx, |workspace, cx| {
                    workspace.show_notification(
                        NotificationId::composite::<DevContainerWarning>(warning.clone()),
                        cx,
                        |cx| cx.new(|cx| MessageNotification::new(warning, cx)),
                    );
                })
            })
            .ok();
    }
}

/// Runs the lifecycle hooks the spec's `waitFor` let through after connecting, as
/// terminal tasks of `window`'s workspace, so their output stays visible. Hooks run in
/// order, the commands of one hook concurrently, and a failure stops the hooks after it.
pub(crate) fn run_deferred_hooks(
    window: WindowHandle<MultiWorkspace>,
    remote_folder: String,
    hooks: Vec<DeferredHook>,
    cx: &mut AsyncApp,
) {
    if hooks.is_empty() {
        return;
    }
    cx.spawn(async move |cx| {
        for hook in hooks {
            let mut completions = Vec::new();
            for command in hook.commands {
                let template = TaskTemplate {
                    label: command.label,
                    command: command.program,
                    args: command.args,
                    cwd: Some(remote_folder.clone()),
                    ..TaskTemplate::default()
                };
                let Some(task) = template.resolve_task("dev-container", &TaskContext::default())
                else {
                    continue;
                };
                let (tx, rx) = futures::channel::oneshot::channel();
                let scheduled = window.update(cx, |multi_workspace, window, cx| {
                    multi_workspace.workspace().update(cx, |workspace, cx| {
                        workspace.schedule_resolved_task_with_completion(
                            TaskSourceKind::UserInput,
                            task,
                            true,
                            move |result, _| {
                                tx.send(result).ok();
                            },
                            window,
                            cx,
                        );
                    })
                });
                if scheduled.is_err() {
                    return;
                }
                completions.push(rx);
            }
            let results = futures::future::join_all(completions).await;
            let succeeded = results
                .into_iter()
                .all(|result| matches!(result, Ok(ScheduledTaskResult::Success)));
            if !succeeded {
                log::error!(
                    "{} failed; skipping the lifecycle hooks after it",
                    hook.name
                );
                return;
            }
        }
    })
    .detach();
}

/// Shows the connection modal on `workspace` with `status`, giving feedback
/// while a slow container-lifecycle step runs. If the modal is not already
/// open it is toggled on; otherwise its status label is updated in place.
/// Best-effort: does nothing if the workspace has gone away.
fn show_lifecycle_status(
    workspace_handle: &WeakEntity<Workspace>,
    options: &DockerConnectionOptions,
    status: &str,
    cx: &mut AsyncWindowContext,
) {
    let connection_options = RemoteConnectionOptions::Docker(options.clone());
    let status = status.to_string();
    workspace_handle
        .update_in(cx, |workspace, window, cx| {
            set_connection_modal_status(workspace, &connection_options, status, window, cx)
        })
        .ok();
}

/// Dismisses the connection modal previously shown by [`show_lifecycle_status`],
/// so a subsequent error prompt isn't left sitting behind the spinner.
fn dismiss_lifecycle_status(workspace_handle: &WeakEntity<Workspace>, cx: &mut AsyncWindowContext) {
    workspace_handle.update(cx, dismiss_connection_modal).ok();
}

/// Rebuilds the dev container described by `options` from scratch and returns
/// fresh connection options pointing at the newly built container.
///
/// The build inputs (host project folder + config) are recovered from the
/// connection's persisted `local_folder`/`config_file` labels, so this works
/// even when the old container has been removed. Because a rebuild mints a new
/// `container_id`, the caller must reconnect with the returned options rather
/// than the originals; the persisted connection row is keyed on the stable
/// labels, so it is reused (not duplicated) across the rebuild.
///
/// Only ever invoked on an explicit user action.
pub(crate) async fn rebuild_dev_container_connection(
    workspace: WeakEntity<Workspace>,
    options: &DockerConnectionOptions,
    cx: &mut AsyncApp,
) -> anyhow::Result<RemoteConnectionOptions> {
    let local_folder = options
        .local_folder
        .clone()
        .context("dev container connection is missing its local_folder label")?;
    let config_file = options
        .config_file
        .clone()
        .context("dev container connection is missing its config_file label")?;
    let local_folder: Arc<Path> = Arc::from(options.host.local_path(&local_folder).as_path());
    let config = DevContainerConfig::from_recovered_paths(
        &local_folder,
        &options.host.local_path(&config_file),
    );

    let context = workspace.update(cx, |workspace, cx| {
        DevContainerContext::for_local_directory(
            local_folder.clone(),
            options.host.clone(),
            workspace,
            cx,
        )
    })?;
    let environment = context.environment(cx).await;

    // The caller reconnects the existing window itself, so every hook runs before.
    let StartedDevContainer { connection, .. } = dev_container::start_dev_container_with_config(
        context,
        Some(config),
        environment,
        BuildMode::Rebuild,
        false,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(Connection::DevContainer(connection).into())
}

/// Starts the dev container backing `options` (a `docker start`), a no-op if it
/// is already running. Used by the connection-failure modal's "Reconnect Dev
/// Container" to bring a stopped container back up before retrying, without
/// disturbing a container that is merely wedged.
///
/// Like the other lifecycle side effects, this only runs on an explicit user
/// action.
pub(crate) async fn start_dev_container(options: &DockerConnectionOptions) -> anyhow::Result<()> {
    dev_container::start_dev_container(&options.container_id, options.use_podman, &options.host)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Restarts the dev container backing `options` in place (a `docker stop`
/// followed by `docker start`), clearing any wedged in-container server state
/// so a subsequent connection can start from a clean slate.
///
/// This must only ever be invoked in response to an explicit user action:
/// restarting a container is side-effecting and the user has to opt into it.
pub(crate) async fn restart_dev_container(options: &DockerConnectionOptions) -> anyhow::Result<()> {
    dev_container::restart_dev_container(&options.container_id, options.use_podman, &options.host)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}
