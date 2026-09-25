use std::sync::Arc;

use dev_container::{DevContainerConfig, DevContainerContext, DevContainerSummary};
use gpui::{
    Action as _, AnyElement, App, Context, DismissEvent, SharedString, Task, WeakEntity, Window,
};
use picker::{Picker, PickerDelegate};
use remote::{EngineHost, RemoteConnectionOptions};
use ui::{Icon, IconName, ListItem, ListItemSpacing, prelude::*};
use workspace::{AppState, Workspace};
use zed_actions::ManageDevContainers;

use crate::dev_container_lifecycle::start_and_open_dev_container;

pub(crate) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(manage_dev_containers);
    })
    .detach();
}

/// Lists the dev containers of the project's engine, like VS Code's Remote
/// Explorer, to open, stop or remove them.
fn manage_dev_containers(
    workspace: &mut Workspace,
    _: &ManageDevContainers,
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
                "Cannot list dev containers",
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
    let this = cx.entity().downgrade();
    cx.spawn_in(window, async move |workspace, cx| {
        let containers = match dev_container::list_dev_containers(use_podman, &engine_host).await {
            Ok(containers) => containers,
            Err(error) => {
                cx.prompt(
                    gpui::PromptLevel::Critical,
                    "Failed to list the dev containers",
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
                    let delegate = DevContainersDelegate {
                        matches: containers.clone(),
                        containers,
                        selected_index: 0,
                        engine_host,
                        use_podman,
                        app_state,
                        workspace: this,
                    };
                    Picker::uniform_list(delegate, window, cx)
                });
            })
            .ok();
    })
    .detach();
}

/// The containers whose name or project folder contain `query`.
fn matching_containers(
    containers: &[DevContainerSummary],
    query: &str,
) -> Vec<DevContainerSummary> {
    let query = query.to_lowercase();
    containers
        .iter()
        .filter(|container| {
            container.name.to_lowercase().contains(&query)
                || container.local_folder.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

struct DevContainersDelegate {
    containers: Vec<DevContainerSummary>,
    matches: Vec<DevContainerSummary>,
    selected_index: usize,
    engine_host: EngineHost,
    use_podman: bool,
    app_state: Arc<AppState>,
    workspace: WeakEntity<Workspace>,
}

impl DevContainersDelegate {
    fn selected(&self) -> Option<&DevContainerSummary> {
        self.matches.get(self.selected_index)
    }

    /// Opens the container's project in it, starting the container if it
    /// stopped.
    fn open(&self, container: DevContainerSummary, window: &mut Window, cx: &mut App) {
        let engine_host = self.engine_host.clone();
        let local_folder = engine_host.local_path(&container.local_folder);
        let config = container
            .config_file
            .as_deref()
            .map(|config_file| {
                DevContainerConfig::from_recovered_paths(
                    &local_folder,
                    &engine_host.local_path(config_file),
                )
            })
            .unwrap_or_else(DevContainerConfig::default_config);
        let app_state = self.app_state.clone();
        self.workspace
            .update(cx, |workspace, cx| {
                let context = DevContainerContext::for_local_directory(
                    Arc::from(local_folder.as_path()),
                    engine_host,
                    workspace,
                    cx,
                );
                cx.spawn_in(window, async move |_, cx| {
                    start_and_open_dev_container(
                        context,
                        config,
                        app_state,
                        "Failed to open Dev Container",
                        cx,
                    )
                    .await;
                })
                .detach();
            })
            .ok();
    }

    /// Stops a running container, or removes a stopped one once confirmed.
    fn stop_or_remove(&self, container: DevContainerSummary, window: &mut Window, cx: &mut App) {
        let engine_host = self.engine_host.clone();
        let use_podman = self.use_podman;
        window
            .spawn(cx, async move |cx| {
                let result = if container.running {
                    dev_container::stop_dev_container(&container.id, use_podman, &engine_host).await
                } else {
                    let confirmed = cx
                        .prompt(
                            gpui::PromptLevel::Warning,
                            &format!("Remove the dev container {}?", container.name),
                            Some(
                                "Anything not stored in the project folder or a volume is \
                                 lost, and opening it again rebuilds the container.",
                            ),
                            &["Remove", "Cancel"],
                        )
                        .await;
                    if !matches!(confirmed, Ok(0)) {
                        return;
                    }
                    dev_container::remove_dev_container(&container.id, use_podman, &engine_host)
                        .await
                };
                if let Err(error) = result {
                    cx.prompt(
                        gpui::PromptLevel::Critical,
                        "Failed to manage the dev container",
                        Some(&error.to_string()),
                        &["OK"],
                    )
                    .await
                    .ok();
                }
            })
            .detach();
    }
}

impl PickerDelegate for DevContainersDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "dev containers"
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.selected_index = ix;
        // The footer's secondary action depends on the container's state.
        cx.notify();
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Search dev containers".into()
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        Some("No dev containers".into())
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.matches = matching_containers(&self.containers, &query);
        self.selected_index = self
            .selected_index
            .min(self.matches.len().saturating_sub(1));
        Task::ready(())
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(container) = self.selected().cloned() else {
            return;
        };
        if secondary {
            self.stop_or_remove(container, window, cx);
        } else {
            self.open(container, window, cx);
        }
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
        let state = if container.running {
            "Running"
        } else {
            "Stopped"
        };
        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(Icon::new(IconName::Box).color(if container.running {
                    Color::Success
                } else {
                    Color::Muted
                }))
                .child(
                    v_flex()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(Label::new(container.name.clone()))
                                .child(
                                    Label::new(state).size(LabelSize::Small).color(Color::Muted),
                                ),
                        )
                        .child(
                            Label::new(container.local_folder.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_footer(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        let secondary_label = if self.selected().is_some_and(|container| container.running) {
            "Stop"
        } else {
            "Remove"
        };
        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .gap_1()
                .justify_end()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(
                    Button::new("open-dev-container", "Open")
                        .key_binding(
                            ui::KeyBinding::for_action(&menu::Confirm, cx)
                                .map(|binding| binding.size(rems_from_px(12_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                        }),
                )
                .child(
                    Button::new("stop-or-remove-dev-container", secondary_label)
                        .key_binding(
                            ui::KeyBinding::for_action(&menu::SecondaryConfirm, cx)
                                .map(|binding| binding.size(rems_from_px(12_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                        }),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use dev_container::DevContainerSummary;

    #[test]
    fn containers_match_by_name_or_project_folder() {
        let container = |name: &str, local_folder: &str| DevContainerSummary {
            id: name.to_string(),
            name: name.to_string(),
            running: true,
            local_folder: local_folder.to_string(),
            config_file: None,
        };
        let containers = [
            container("app_devcontainer", "/src/App"),
            container("zed_dev", "/src/zed"),
        ];
        let names = |query: &str| {
            super::matching_containers(&containers, query)
                .into_iter()
                .map(|container| container.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(""), ["app_devcontainer", "zed_dev"]);
        assert_eq!(names("/src/app"), ["app_devcontainer"]);
        assert_eq!(names("ZED"), ["zed_dev"]);
        assert!(names("missing").is_empty());
    }
}
