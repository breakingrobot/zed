use std::sync::Arc;

use dev_container::{DevContainerConfig, DevContainerContext};
use gpui::{
    AnyElement, App, AppContext as _, Context, DismissEvent, SharedString, Task, WeakEntity, Window,
};
use picker::{Picker, PickerDelegate};
use remote::EngineHost;
use ui::{Icon, IconName, ListItem, ListItemSpacing, prelude::*};
use workspace::{
    Workspace,
    notifications::{NotificationId, simple_message_notification::MessageNotification},
};
use zed_actions::CloneRepositoryInContainerVolume;

use crate::dev_container_lifecycle::{prompt_start_error, start_and_open_dev_container};

pub(crate) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(
            |workspace, _: &CloneRepositoryInContainerVolume, window, cx| {
                let handle = cx.entity().downgrade();
                workspace.toggle_modal(window, cx, |window, cx| {
                    Picker::uniform_list(RepositoryUrlDelegate::new(handle), window, cx)
                });
            },
        );
    })
    .detach();
}

/// Takes the URL of the repository to clone, typed as the picker's query.
struct RepositoryUrlDelegate {
    workspace: WeakEntity<Workspace>,
    url: String,
}

impl RepositoryUrlDelegate {
    fn new(workspace: WeakEntity<Workspace>) -> Self {
        Self {
            workspace,
            url: String::new(),
        }
    }
}

impl PickerDelegate for RepositoryUrlDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "clone repository in container volume"
    }

    fn match_count(&self) -> usize {
        usize::from(!self.url.is_empty())
    }

    fn selected_index(&self) -> usize {
        0
    }

    fn set_selected_index(&mut self, _: usize, _: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Repository URL, e.g. https://github.com/owner/repository.git".into()
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        None
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.url = query.trim().to_string();
        Task::ready(())
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if self.url.is_empty() {
            return;
        }
        let url = self.url.clone();
        self.workspace
            .update(cx, |workspace, cx| {
                clone_and_open(workspace, url, window, cx);
            })
            .ok();
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
        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(Icon::new(IconName::Download).color(Color::Muted))
                .child(Label::new(format!(
                    "Clone {} in a Container Volume",
                    self.url
                )))
                .into_any_element(),
        )
    }
}

/// Clones `url` into a volume of this machine's container engine, starts its dev
/// container and opens it in a new window.
fn clone_and_open(
    workspace: &mut Workspace,
    url: String,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    struct CloningRepository;

    let notification_id = NotificationId::unique::<CloningRepository>();
    workspace.show_notification(notification_id.clone(), cx, |cx| {
        cx.new(|cx| {
            MessageNotification::new(
                format!("Cloning {url} into a container volume and starting its dev container…"),
                cx,
            )
        })
    });
    let use_podman = dev_container::use_podman(cx);
    let app_state = workspace.app_state().clone();
    let fs = app_state.fs.clone();
    cx.spawn_in(window, async move |workspace, cx| {
        let prepared = async {
            let project_directory = dev_container::clone_repository_in_volume(&url, use_podman)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            let mut config = None;
            for candidate in [
                DevContainerConfig::default_config(),
                DevContainerConfig::root_config(),
            ] {
                if fs
                    .is_file(&project_directory.join(&candidate.config_path))
                    .await
                {
                    config = Some(candidate);
                    break;
                }
            }
            let config = config.ok_or_else(|| {
                anyhow::anyhow!(
                    "The repository has no .devcontainer/devcontainer.json or .devcontainer.json."
                )
            })?;
            let context = workspace.update(cx, |workspace, cx| {
                DevContainerContext::for_local_directory(
                    Arc::from(project_directory.as_path()),
                    EngineHost::Local,
                    workspace,
                    cx,
                )
            })?;
            anyhow::Ok((context, config))
        }
        .await;
        let error_title = "Failed to clone the repository in a container volume";
        match prepared {
            Ok((context, config)) => {
                start_and_open_dev_container(context, config, app_state, error_title, cx).await;
            }
            Err(error) => {
                log::error!("Failed to clone {url} in a container volume: {error:#}");
                prompt_start_error(cx, error_title, format!("{error:#}"), None).await;
            }
        }
        workspace
            .update(cx, |workspace, cx| {
                workspace.dismiss_notification(&notification_id, cx)
            })
            .ok();
    })
    .detach();
}
