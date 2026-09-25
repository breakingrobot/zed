use std::{path::PathBuf, sync::Arc};

use dev_container::RunningContainer;
use gpui::{AnyElement, App, Context, DismissEvent, SharedString, Task, Window};
use picker::{Picker, PickerDelegate};
use remote::{DockerConnectionOptions, EngineHost, RemoteConnectionOptions};
use ui::{Icon, IconName, ListItem, ListItemSpacing, prelude::*};
use workspace::{AppState, OpenOptions, Workspace};
use zed_actions::AttachToRunningContainer;

use crate::remote_connections::open_remote_project;

pub(crate) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(attach_to_running_container);
    })
    .detach();
}

/// Lists the running containers of the project's engine, like VS Code's "Attach
/// to Running Container…".
fn attach_to_running_container(
    workspace: &mut Workspace,
    _: &AttachToRunningContainer,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let project = workspace.project().read(cx);
    let engine_host = match project.remote_connection_options(cx) {
        Some(RemoteConnectionOptions::Docker(options)) => Some(options.host),
        _ => dev_container::engine_host_for_project(project, cx),
    };
    let Some(engine_host) = engine_host else {
        let reason = dev_container::unsupported_reason(project, cx)
            .unwrap_or("Containers can't be reached from this project.");
        cx.spawn_in(window, async move |_, cx| {
            cx.prompt(
                gpui::PromptLevel::Critical,
                "Cannot attach to a container",
                Some(reason),
                &["OK"],
            )
            .await
            .ok();
        })
        .detach();
        return;
    };
    let use_podman = dev_container::use_podman(cx);
    let app_state = workspace.app_state().clone();
    cx.spawn_in(window, async move |workspace, cx| {
        let containers = match dev_container::running_containers(use_podman, &engine_host).await {
            Ok(containers) => containers,
            Err(error) => {
                cx.prompt(
                    gpui::PromptLevel::Critical,
                    "Failed to list the running containers",
                    Some(&error.to_string()),
                    &["OK"],
                )
                .await
                .ok();
                return;
            }
        };
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    let delegate = RunningContainersDelegate {
                        containers: containers.clone(),
                        matches: containers,
                        selected_index: 0,
                        engine_host,
                        use_podman,
                        app_state,
                    };
                    Picker::uniform_list(delegate, window, cx)
                });
            })
            .ok();
    })
    .detach();
}

struct RunningContainersDelegate {
    containers: Vec<RunningContainer>,
    matches: Vec<RunningContainer>,
    selected_index: usize,
    engine_host: EngineHost,
    use_podman: bool,
    app_state: Arc<AppState>,
}

impl PickerDelegate for RunningContainersDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "running containers"
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, _: &mut Context<Picker<Self>>) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Attach to a running container".into()
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        Some("No running containers".into())
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.to_lowercase();
        self.matches = self
            .containers
            .iter()
            .filter(|container| {
                container.name.to_lowercase().contains(&query)
                    || container.image.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        self.selected_index = self
            .selected_index
            .min(self.matches.len().saturating_sub(1));
        Task::ready(())
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(container) = self.matches.get(self.selected_index).cloned() else {
            return;
        };
        let engine_host = self.engine_host.clone();
        let use_podman = self.use_podman;
        let app_state = self.app_state.clone();
        cx.spawn_in(window, async move |_, cx| {
            let target =
                match dev_container::attach_target(&container.id, use_podman, &engine_host).await {
                    Ok(target) => target,
                    Err(error) => {
                        cx.prompt(
                            gpui::PromptLevel::Critical,
                            "Failed to attach to the container",
                            Some(&error.to_string()),
                            &["OK"],
                        )
                        .await
                        .ok();
                        return;
                    }
                };
            let options = attached_connection_options(
                &container,
                target.remote_user,
                use_podman,
                engine_host,
            );
            let result = open_remote_project(
                RemoteConnectionOptions::Docker(options),
                vec![PathBuf::from(target.working_directory)],
                app_state,
                OpenOptions::default(),
                cx,
            )
            .await;
            if let Err(error) = result {
                log::error!(
                    "Failed to attach to container {}: {error:#}",
                    container.name
                );
                cx.prompt(
                    gpui::PromptLevel::Critical,
                    "Failed to attach to the container",
                    Some(&format!("{error:#}")),
                    &["OK"],
                )
                .await
                .ok();
            }
        })
        .detach();
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let container = self.matches.get(ix)?;
        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(Icon::new(IconName::Box).color(Color::Muted))
                .child(
                    h_flex()
                        .gap_2()
                        .child(Label::new(container.name.clone()))
                        .child(
                            Label::new(container.image.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// A connection to a container Zed didn't create: without a configuration to
/// rebuild it from, or ports and shutdown to manage, like VS Code's attached
/// containers.
fn attached_connection_options(
    container: &RunningContainer,
    remote_user: String,
    use_podman: bool,
    host: EngineHost,
) -> DockerConnectionOptions {
    DockerConnectionOptions {
        name: container.name.clone(),
        container_id: container.id.clone(),
        remote_user,
        use_podman,
        host,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use dev_container::RunningContainer;
    use remote::{EngineHost, ShutdownAction};

    #[test]
    fn attached_containers_have_no_dev_container_identity() {
        let options = super::attached_connection_options(
            &RunningContainer {
                id: "4f2a".to_string(),
                name: "db".to_string(),
                image: "postgres:16".to_string(),
            },
            "postgres".to_string(),
            true,
            EngineHost::Local,
        );
        assert_eq!(options.container_id, "4f2a");
        assert_eq!(options.name, "db");
        assert_eq!(options.remote_user, "postgres");
        assert!(options.use_podman);
        assert_eq!(options.dev_container_labels(), None);
        assert_eq!(options.shutdown_action, ShutdownAction::None);
    }
}
