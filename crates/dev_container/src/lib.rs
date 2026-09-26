use std::path::Path;

use fs::Fs;
use gpui::AppContext;
use gpui::Entity;
use gpui::Task;
use gpui::WeakEntity;
use http_client::anyhow;
use picker::Picker;
use picker::PickerDelegate;
use project::{Project, ProjectEnvironment};
use remote::{EngineHost, RemoteConnectionOptions, SshEngineHost};
use settings::RegisterSetting;
use settings::Settings;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::fmt::Display;
use std::sync::Arc;
use ui::ActiveTheme;
use ui::Button;
use ui::Clickable;
use ui::FluentBuilder;
use ui::KeyBinding;
use ui::StatefulInteractiveElement;
use ui::Switch;
use ui::ToggleState;
use ui::Tooltip;
use ui::h_flex;
use ui::rems_from_px;
use ui::v_flex;
use util::paths::PathStyle;
use util::shell::Shell;

use gpui::{Action, DismissEvent, EventEmitter, FocusHandle, Focusable, RenderOnce};
use serde::Deserialize;
use ui::{
    AnyElement, App, Color, CommonAnimationExt, Context, Headline, HeadlineSize, Icon, IconName,
    InteractiveElement, IntoElement, Label, ListItem, ListSeparator, ModalHeader, Navigable,
    NavigableEntry, ParentElement, Render, Styled, StyledExt, Toggleable, Window, div, rems,
};
use util::ResultExt;
use util::rel_path::RelPath;
use workspace::{ModalView, Workspace, with_active_or_new_workspace};

use http_client::HttpClient;

mod command_json;
mod devcontainer_api;
mod devcontainer_json;
mod devcontainer_manifest;
mod docker;
mod features;
mod host_files;
mod oci;
mod workspace_volume;

use devcontainer_api::read_default_devcontainer_configuration;

use crate::devcontainer_api::apply_devcontainer_template;
use crate::oci::get_deserializable_oci_blob;
use crate::oci::get_latest_oci_manifest;
use crate::oci::get_oci_token;

pub use devcontainer_api::{
    AttachTarget, BuildMode, DeferredCommand, DeferredHook, DevContainerConfig, DevContainerError,
    DevContainerOrigin, DevContainerSummary, RunningContainer, StartedDevContainer, attach_target,
    container_logs, dev_container_origin, find_configs_in_snapshot, find_devcontainer_configs,
    list_dev_containers, rebuild_dev_container, remove_dev_container, restart_dev_container,
    running_containers, shut_down_dev_container, shutdown_command, start_dev_container,
    start_dev_container_with_config, stop_dev_container,
};
pub use workspace_volume::clone_repository_in_volume;

/// Converts a string to a safe environment variable name.
///
/// Mirrors the CLI's `getSafeId` in `containerFeatures.ts`:
/// replaces non-alphanumeric/underscore characters with `_`, replaces a
/// leading sequence of digits/underscores with a single `_`, and uppercases.
pub(crate) fn safe_id_lower(input: &str) -> String {
    get_safe_id(input).to_lowercase()
}
pub(crate) fn safe_id_upper(input: &str) -> String {
    get_safe_id(input).to_uppercase()
}
fn get_safe_id(input: &str) -> String {
    let replaced: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let without_leading = replaced.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
    let result = if without_leading.len() < replaced.len() {
        format!("_{}", without_leading)
    } else {
        replaced
    };
    result
}

pub struct DevContainerContext {
    /// The project's root folder, as the machine running Zed sees it.
    pub project_directory: Arc<Path>,
    /// Where the container engine runs, and where the project's sources live.
    pub engine_host: EngineHost,
    pub use_podman: bool,
    pub use_buildkit: Option<bool>,
    /// The user's dotfiles, installed in new containers.
    pub dotfiles: Option<Dotfiles>,
    /// A JSON object of secrets that lifecycle commands get as environment
    /// variables.
    pub secrets_file: Option<std::path::PathBuf>,
    /// The volume that holds the project's sources, when `project_directory` is
    /// only a copy of them (see [`clone_repository_in_volume`]).
    pub workspace_volume: Option<String>,
    /// Whether the engine runs on another machine than the engine host, which
    /// then can't share its files and sockets with containers.
    pub remote_engine: bool,
    /// What Zed learned about engines and containers earlier in this session.
    pub session_cache: SessionCache,
    pub fs: Arc<dyn Fs>,
    pub http_client: Arc<dyn HttpClient>,
    pub environment: WeakEntity<ProjectEnvironment>,
}

impl DevContainerContext {
    pub fn from_workspace(workspace: &Workspace, cx: &App) -> Option<Self> {
        let project = workspace.project().read(cx);
        let root = project.active_project_directory(cx)?;
        let engine_host = engine_host_for_project(project, cx)?;
        let project_directory = if engine_host.is_local() {
            root
        } else {
            let host_root = root.to_string_lossy().replace('\\', "/");
            Arc::from(engine_host.local_path(&host_root).as_path())
        };
        Some(Self::for_local_directory(
            project_directory,
            engine_host,
            workspace,
            cx,
        ))
    }

    /// Builds a context for an explicit `project_directory` on `engine_host`, as
    /// the machine running Zed sees it, rather than deriving it from the current
    /// project's active worktree. The active worktree's path isn't usable when the
    /// current project itself is a dev container connection, since its worktree
    /// paths are in-container paths rather than host paths (e.g. when rebuilding
    /// a dev container from within its own window).
    pub fn for_local_directory(
        project_directory: Arc<Path>,
        engine_host: EngineHost,
        workspace: &Workspace,
        cx: &App,
    ) -> Self {
        let settings = DevContainerSettings::get_global(cx);
        let workspace_volume = engine_host
            .is_local()
            .then(|| workspace_volume::workspace_volume_of(&project_directory))
            .flatten();
        Self {
            project_directory,
            engine_host,
            use_podman: settings.use_podman,
            use_buildkit: settings.use_buildkit,
            dotfiles: settings.dotfiles.clone(),
            secrets_file: settings.secrets_file.clone(),
            workspace_volume,
            remote_engine: false,
            session_cache: SessionCache::global(cx),
            fs: workspace.app_state().fs.clone(),
            http_client: cx.http_client().clone(),
            environment: workspace.project().read(cx).environment().downgrade(),
        }
    }

    /// The environment of the engine host, where `${localEnv:…}` variables come from.
    pub async fn environment(&self, cx: &mut impl AppContext) -> HashMap<String, String> {
        if !self.engine_host.is_local() {
            return host_environment(&self.engine_host).await;
        }
        let Ok(task) = self.environment.update(cx, |this, cx| {
            this.local_directory_environment(&Shell::System, self.project_directory.clone(), cx)
        }) else {
            return HashMap::default();
        };
        task.await
            .map(|env| env.into_iter().collect::<std::collections::HashMap<_, _>>())
            .unwrap_or_default()
    }
}

async fn host_environment(host: &EngineHost) -> HashMap<String, String> {
    host.login_environment()
        .await
        .map(|environment| environment.into_iter().collect())
        .unwrap_or_else(|error| {
            log::error!("{error:#}");
            HashMap::default()
        })
}

/// Where the commands that created or started the dev container of `local_folder`
/// and `config_file`, as in its labels, are recorded with their output. Each dev
/// container has its own, so starting another one doesn't replace or mix into it.
pub fn dev_container_log_path(local_folder: &str, config_file: &str) -> std::path::PathBuf {
    use sha2::{Digest as _, Sha256};

    let digest = Sha256::digest(format!("{local_folder}\0{config_file}"));
    let id: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    paths::logs_dir().join(format!("dev_container-{id}.log"))
}

/// [`dev_container_log_path`] for starting `config` for `context`'s project.
pub fn start_log_path(
    context: &DevContainerContext,
    config: &DevContainerConfig,
) -> std::path::PathBuf {
    let host = &context.engine_host;
    let local_folder = devcontainer_manifest::normalize_label_path(
        &host.host_path(&context.project_directory),
        host.is_windows(),
    );
    let config_file = devcontainer_manifest::normalize_label_path(
        &host.host_path(&context.project_directory.join(&config.config_path)),
        host.is_windows(),
    );
    dev_container_log_path(&local_folder, &config_file)
}

static LAST_START_LOG_PATH: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

/// The log of the dev container that was started last, for windows that aren't
/// connected to one.
pub fn last_start_log_path() -> Option<std::path::PathBuf> {
    LAST_START_LOG_PATH
        .lock()
        .ok()
        .and_then(|path| path.clone())
}

pub(crate) fn set_last_start_log_path(path: std::path::PathBuf) {
    if let Ok(mut last) = LAST_START_LOG_PATH.lock() {
        *last = Some(path);
    }
}

/// What Zed learned about container engines and containers earlier in this
/// session, so reopening a dev container doesn't ask again: whether an engine
/// has BuildKit, and the environment of a container's user shell.
#[derive(Clone, Default)]
pub struct SessionCache(Arc<std::sync::Mutex<SessionCacheState>>);

#[derive(Default)]
struct SessionCacheState {
    buildkit: HashMap<(String, EngineHost), bool>,
    user_environments: HashMap<UserEnvironmentKey, HashMap<String, String>>,
}

/// A container's user shell environment stays valid until the container
/// restarts.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct UserEnvironmentKey {
    pub(crate) container_id: String,
    pub(crate) started_at: Option<String>,
    pub(crate) remote_user: String,
}

impl gpui::Global for SessionCache {}

impl SessionCache {
    /// The cache of this session, or an empty one if the dev container crate
    /// wasn't initialized.
    pub fn global(cx: &App) -> Self {
        cx.try_global::<SessionCache>().cloned().unwrap_or_default()
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut SessionCacheState) -> R) -> Option<R> {
        match self.0.lock() {
            Ok(mut state) => Some(f(&mut state)),
            Err(error) => {
                log::error!("The dev container session cache is unusable: {error}");
                None
            }
        }
    }

    pub(crate) fn buildkit(&self, docker_cli: &str, host: &EngineHost) -> Option<bool> {
        self.with_state(|state| {
            state
                .buildkit
                .get(&(docker_cli.to_string(), host.clone()))
                .copied()
        })
        .flatten()
    }

    pub(crate) fn set_buildkit(&self, docker_cli: &str, host: &EngineHost, buildkit: bool) {
        self.with_state(|state| {
            state
                .buildkit
                .insert((docker_cli.to_string(), host.clone()), buildkit)
        });
    }

    pub(crate) fn user_environment(
        &self,
        key: &UserEnvironmentKey,
    ) -> Option<HashMap<String, String>> {
        self.with_state(|state| state.user_environments.get(key).cloned())
            .flatten()
    }

    pub(crate) fn set_user_environment(
        &self,
        key: UserEnvironmentKey,
        environment: HashMap<String, String>,
    ) {
        self.with_state(|state| {
            // Environments of the container before it restarted are stale.
            state.user_environments.retain(|cached, _| {
                cached.container_id != key.container_id || cached.started_at == key.started_at
            });
            state.user_environments.insert(key, environment)
        });
    }

    /// Forgets what was learned about a container that was removed, e.g. to be
    /// rebuilt.
    pub fn forget_container(&self, container_id: &str) {
        self.with_state(|state| {
            state
                .user_environments
                .retain(|cached, _| cached.container_id != container_id)
        });
    }
}

/// A dotfiles repository to install in new dev containers, like VS Code's
/// `dotfiles.*` settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dotfiles {
    /// A Git URL, or `owner/repository` on GitHub.
    pub repository: String,
    /// The command that installs the dotfiles, run in the clone.
    pub install_command: Option<String>,
    /// Where the repository is cloned in the container.
    pub target_path: Option<String>,
}

#[derive(RegisterSetting)]
struct DevContainerSettings {
    use_podman: bool,
    use_wslc: bool,
    use_buildkit: Option<bool>,
    dotfiles: Option<Dotfiles>,
    secrets_file: Option<std::path::PathBuf>,
}

/// Where the container engine of `project`'s dev containers runs: where its
/// sources are. `None` when dev containers can't be opened from it.
pub fn engine_host_for_project(project: &Project, cx: &App) -> Option<EngineHost> {
    engine_host_or_unsupported_reason(project, cx).ok()
}

/// Why dev containers can't be opened from `project`, to tell the user; `None`
/// when they can.
pub fn unsupported_reason(project: &Project, cx: &App) -> Option<&'static str> {
    engine_host_or_unsupported_reason(project, cx).err()
}

fn engine_host_or_unsupported_reason(
    project: &Project,
    cx: &App,
) -> Result<EngineHost, &'static str> {
    // A guest's project has no connection of its own: its files are the host's.
    if project.is_via_collab() {
        return Err("Dev containers can't be opened from a project shared with you.");
    }
    match project.remote_connection_options(cx) {
        None => Ok(EngineHost::Local),
        // Zed reaches the distribution's files over `\\wsl.localhost`.
        Some(RemoteConnectionOptions::Wsl(options)) => Ok(EngineHost::Wsl(options)),
        Some(RemoteConnectionOptions::Ssh(options)) => {
            // Commands and paths on the host are POSIX.
            let is_posix = project
                .remote_client()
                .is_some_and(|client| client.read(cx).path_style() == PathStyle::Unix);
            if is_posix {
                Ok(EngineHost::Ssh(SshEngineHost::from(&options)))
            } else {
                Err("Dev containers over SSH need a Linux or macOS host.")
            }
        }
        Some(RemoteConnectionOptions::Docker(_)) => {
            Err("This project is already open in a dev container.")
        }
        #[allow(unreachable_patterns)]
        Some(_) => Err("Dev containers can't be opened from this kind of remote project."),
    }
}

pub fn use_podman(cx: &App) -> bool {
    DevContainerSettings::get_global(cx).use_podman
}

impl Settings for DevContainerSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self {
            use_podman: content.remote.use_podman.unwrap_or(false),
            use_wslc: content.remote.dev_container_use_wslc.unwrap_or(false),
            use_buildkit: content.remote.dev_container_use_buildkit,
            secrets_file: content
                .remote
                .dev_container_secrets_file
                .as_deref()
                .filter(|path| !path.trim().is_empty())
                .map(|path| match path.strip_prefix("~/") {
                    Some(relative) => util::paths::home_dir().join(relative),
                    None => std::path::PathBuf::from(path),
                }),
            dotfiles: content
                .remote
                .dev_container_dotfiles_repository
                .clone()
                .filter(|repository| !repository.trim().is_empty())
                .map(|repository| Dotfiles {
                    repository,
                    install_command: content
                        .remote
                        .dev_container_dotfiles_install_command
                        .clone()
                        .filter(|command| !command.trim().is_empty()),
                    target_path: content
                        .remote
                        .dev_container_dotfiles_target_path
                        .clone()
                        .filter(|path| !path.trim().is_empty()),
                }),
        }
    }
}

#[derive(PartialEq, Clone, Deserialize, Default, Action)]
#[action(namespace = projects)]
#[serde(deny_unknown_fields)]
struct InitializeDevContainer;

pub fn init(cx: &mut App) {
    cx.set_global(SessionCache::default());
    // The connections to dev containers read the secrets file from here.
    let secrets_file = |cx: &App| {
        remote::DevContainerSecretsFile(DevContainerSettings::get_global(cx).secrets_file.clone())
    };
    cx.set_global(secrets_file(cx));
    remote::set_use_wslc(DevContainerSettings::get_global(cx).use_wslc);
    cx.observe_global::<settings::SettingsStore>(move |cx| {
        let secrets_file = secrets_file(cx);
        cx.set_global(secrets_file);
        remote::set_use_wslc(DevContainerSettings::get_global(cx).use_wslc);
    })
    .detach();
    cx.on_action(|_: &InitializeDevContainer, cx| {
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            let weak_entity = cx.weak_entity();
            workspace.toggle_modal(window, cx, |window, cx| {
                DevContainerModal::new(weak_entity, window, cx)
            });
        });
    });
}

#[derive(Clone)]
struct TemplateEntry {
    template: DevContainerTemplate,
    options_selected: HashMap<String, String>,
    current_option_index: usize,
    current_option: Option<TemplateOptionSelection>,
    features_selected: HashSet<DevContainerFeature>,
}

#[derive(Clone)]
struct FeatureEntry {
    feature: DevContainerFeature,
    toggle_state: ToggleState,
}

#[derive(Clone)]
struct TemplateOptionSelection {
    option_name: String,
    description: String,
    navigable_options: Vec<(String, NavigableEntry)>,
}

impl Eq for TemplateEntry {}
impl PartialEq for TemplateEntry {
    fn eq(&self, other: &Self) -> bool {
        self.template == other.template
    }
}
impl Debug for TemplateEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateEntry")
            .field("template", &self.template)
            .finish()
    }
}

impl Eq for FeatureEntry {}
impl PartialEq for FeatureEntry {
    fn eq(&self, other: &Self) -> bool {
        self.feature == other.feature
    }
}

impl Debug for FeatureEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeatureEntry")
            .field("feature", &self.feature)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DevContainerState {
    Initial,
    QueryingTemplates,
    TemplateQueryReturned(Result<Vec<TemplateEntry>, String>),
    QueryingFeatures(TemplateEntry),
    FeaturesQueryReturned(TemplateEntry),
    UserOptionsSpecifying(TemplateEntry),
    ConfirmingWriteDevContainer(TemplateEntry),
    TemplateWriteFailed(DevContainerError),
}

#[derive(Debug, Clone)]
enum DevContainerMessage {
    SearchTemplates,
    TemplatesRetrieved(Vec<DevContainerTemplate>),
    ErrorRetrievingTemplates(String),
    TemplateSelected(TemplateEntry),
    TemplateOptionsSpecified(TemplateEntry),
    TemplateOptionsCompleted(TemplateEntry),
    FeaturesRetrieved(Vec<DevContainerFeature>),
    FeaturesSelected(TemplateEntry),
    NeedConfirmWriteDevContainer(TemplateEntry),
    ConfirmWriteDevContainer(TemplateEntry),
    FailedToWriteTemplate(DevContainerError),
    GoBack,
}

struct DevContainerModal {
    workspace: WeakEntity<Workspace>,
    picker: Option<Entity<Picker<TemplatePickerDelegate>>>,
    features_picker: Option<Entity<Picker<FeaturePickerDelegate>>>,
    focus_handle: FocusHandle,
    confirm_entry: NavigableEntry,
    back_entry: NavigableEntry,
    state: DevContainerState,
}

struct TemplatePickerDelegate {
    selected_index: usize,
    placeholder_text: String,
    stateful_modal: WeakEntity<DevContainerModal>,
    candidate_templates: Vec<TemplateEntry>,
    matching_indices: Vec<usize>,
    on_confirm: Box<
        dyn FnMut(
            TemplateEntry,
            &mut DevContainerModal,
            &mut Window,
            &mut Context<DevContainerModal>,
        ),
    >,
}

impl TemplatePickerDelegate {
    fn new(
        placeholder_text: String,
        stateful_modal: WeakEntity<DevContainerModal>,
        elements: Vec<TemplateEntry>,
        on_confirm: Box<
            dyn FnMut(
                TemplateEntry,
                &mut DevContainerModal,
                &mut Window,
                &mut Context<DevContainerModal>,
            ),
        >,
    ) -> Self {
        Self {
            selected_index: 0,
            placeholder_text,
            stateful_modal,
            candidate_templates: elements,
            matching_indices: Vec::new(),
            on_confirm,
        }
    }
}

impl PickerDelegate for TemplatePickerDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "dev container template picker"
    }

    fn match_count(&self) -> usize {
        self.matching_indices.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<picker::Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        self.placeholder_text.clone().into()
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        _cx: &mut Context<picker::Picker<Self>>,
    ) -> gpui::Task<()> {
        self.matching_indices = self
            .candidate_templates
            .iter()
            .enumerate()
            .filter(|(_, template_entry)| {
                template_entry
                    .template
                    .id
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                    || template_entry
                        .template
                        .name
                        .to_lowercase()
                        .contains(&query.to_lowercase())
            })
            .map(|(ix, _)| ix)
            .collect();

        self.selected_index = std::cmp::min(
            self.selected_index,
            self.matching_indices.len().saturating_sub(1),
        );
        Task::ready(())
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<picker::Picker<Self>>,
    ) {
        let fun = &mut self.on_confirm;

        if self.matching_indices.is_empty() {
            return;
        }
        self.stateful_modal
            .update(cx, |modal, cx| {
                let Some(confirmed_entry) = self
                    .matching_indices
                    .get(self.selected_index)
                    .and_then(|ix| self.candidate_templates.get(*ix))
                else {
                    log::error!("Selected index not in range of known matches");
                    return;
                };
                fun(confirmed_entry.clone(), modal, window, cx);
            })
            .ok();
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<picker::Picker<Self>>) {
        self.stateful_modal
            .update(cx, |modal, cx| {
                modal.dismiss(&menu::Cancel, window, cx);
            })
            .ok();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<picker::Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let Some(template_entry) = self.candidate_templates.get(self.matching_indices[ix]) else {
            return None;
        };
        Some(
            ListItem::new("li-template-match")
                .inset(true)
                .spacing(ui::ListItemSpacing::Sparse)
                .start_slot(Icon::new(IconName::Box))
                .toggle_state(selected)
                .child(Label::new(template_entry.template.name.clone()))
                .into_any_element(),
        )
    }

    fn render_footer(
        &self,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .gap_1()
                .justify_start()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(
                    Button::new("run-action", "Continue")
                        .key_binding(
                            KeyBinding::for_action(&menu::Confirm, cx)
                                .map(|kb| kb.size(rems_from_px(12_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                        }),
                )
                .into_any_element(),
        )
    }
}

struct FeaturePickerDelegate {
    selected_index: usize,
    placeholder_text: String,
    stateful_modal: WeakEntity<DevContainerModal>,
    candidate_features: Vec<FeatureEntry>,
    template_entry: TemplateEntry,
    matching_indices: Vec<usize>,
    on_confirm: Box<
        dyn FnMut(
            TemplateEntry,
            &mut DevContainerModal,
            &mut Window,
            &mut Context<DevContainerModal>,
        ),
    >,
}

impl FeaturePickerDelegate {
    fn new(
        placeholder_text: String,
        stateful_modal: WeakEntity<DevContainerModal>,
        candidate_features: Vec<FeatureEntry>,
        template_entry: TemplateEntry,
        on_confirm: Box<
            dyn FnMut(
                TemplateEntry,
                &mut DevContainerModal,
                &mut Window,
                &mut Context<DevContainerModal>,
            ),
        >,
    ) -> Self {
        Self {
            selected_index: 0,
            placeholder_text,
            stateful_modal,
            candidate_features,
            template_entry,
            matching_indices: Vec::new(),
            on_confirm,
        }
    }
}

impl PickerDelegate for FeaturePickerDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "dev container feature picker"
    }

    fn match_count(&self) -> usize {
        self.matching_indices.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        self.placeholder_text.clone().into()
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.matching_indices = self
            .candidate_features
            .iter()
            .enumerate()
            .filter(|(_, feature_entry)| {
                feature_entry
                    .feature
                    .id
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                    || feature_entry
                        .feature
                        .name
                        .to_lowercase()
                        .contains(&query.to_lowercase())
            })
            .map(|(ix, _)| ix)
            .collect();
        self.selected_index = std::cmp::min(
            self.selected_index,
            self.matching_indices.len().saturating_sub(1),
        );
        Task::ready(())
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if secondary {
            self.stateful_modal
                .update(cx, |modal, cx| {
                    (self.on_confirm)(self.template_entry.clone(), modal, window, cx)
                })
                .ok();
        } else {
            if self.matching_indices.is_empty() {
                return;
            }
            let Some(current) = self
                .matching_indices
                .get(self.selected_index)
                .and_then(|ix| self.candidate_features.get_mut(*ix))
            else {
                log::error!("Selected index not in range of matches");
                return;
            };
            current.toggle_state = match current.toggle_state {
                ToggleState::Selected => {
                    self.template_entry
                        .features_selected
                        .remove(&current.feature);
                    ToggleState::Unselected
                }
                _ => {
                    self.template_entry
                        .features_selected
                        .insert(current.feature.clone());
                    ToggleState::Selected
                }
            };
        }
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.stateful_modal
            .update(cx, |modal, cx| {
                modal.dismiss(&menu::Cancel, window, cx);
            })
            .ok();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let feature_entry = self.candidate_features[self.matching_indices[ix]].clone();

        Some(
            ListItem::new("li-what")
                .inset(true)
                .toggle_state(selected)
                .start_slot(Switch::new(
                    feature_entry.feature.id.clone(),
                    feature_entry.toggle_state,
                ))
                .child(Label::new(feature_entry.feature.name))
                .into_any_element(),
        )
    }

    fn render_footer(
        &self,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .gap_1()
                .justify_start()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(
                    Button::new("run-action", "Select Feature")
                        .key_binding(
                            KeyBinding::for_action(&menu::Confirm, cx)
                                .map(|kb| kb.size(rems_from_px(12_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                        }),
                )
                .child(
                    Button::new("run-action-secondary", "Confirm Selections")
                        .key_binding(
                            KeyBinding::for_action(&menu::SecondaryConfirm, cx)
                                .map(|kb| kb.size(rems_from_px(12_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                        }),
                )
                .into_any_element(),
        )
    }
}

impl DevContainerModal {
    fn new(workspace: WeakEntity<Workspace>, _window: &mut Window, cx: &mut App) -> Self {
        DevContainerModal {
            workspace,
            picker: None,
            features_picker: None,
            state: DevContainerState::Initial,
            focus_handle: cx.focus_handle(),
            confirm_entry: NavigableEntry::focusable(cx),
            back_entry: NavigableEntry::focusable(cx),
        }
    }

    fn render_initial(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut view = Navigable::new(
            div()
                .p_1()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new().child(
                            Headline::new("Create Dev Container").size(HeadlineSize::XSmall),
                        ),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.confirm_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::SearchTemplates, window, cx);
                        }))
                        .child(
                            ListItem::new("li-search-containers")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(
                                    Icon::new(IconName::MagnifyingGlass).color(Color::Muted),
                                )
                                .toggle_state(
                                    self.confirm_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(
                                        DevContainerMessage::SearchTemplates,
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                }))
                                .child(Label::new("Search for Dev Container Templates")),
                        ),
                )
                .into_any_element(),
        );
        view = view.entry(self.confirm_entry.clone());
        view.render(window, cx).into_any_element()
    }

    fn render_error(
        &self,
        error_title: String,
        error: impl Display,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .p_1()
            .child(div().track_focus(&self.focus_handle).child(
                ModalHeader::new().child(Headline::new(error_title).size(HeadlineSize::XSmall)),
            ))
            .child(ListSeparator)
            .child(
                v_flex()
                    .child(Label::new(format!("{}", error)))
                    .whitespace_normal(),
            )
            .into_any_element()
    }

    fn render_retrieved_templates(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(picker) = &self.picker {
            let picker_element = div()
                .track_focus(&self.focus_handle(cx))
                .child(picker.clone().into_any_element())
                .into_any_element();
            picker.focus_handle(cx).focus(window, cx);
            picker_element
        } else {
            div().into_any_element()
        }
    }

    fn render_user_options_specifying(
        &self,
        template_entry: TemplateEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(next_option_entries) = &template_entry.current_option else {
            return div().into_any_element();
        };
        let mut view = Navigable::new(
            div()
                .child(
                    div()
                        .id("title")
                        .tooltip(Tooltip::text(next_option_entries.description.clone()))
                        .track_focus(&self.focus_handle)
                        .child(
                            ModalHeader::new()
                                .child(
                                    Headline::new("Template Option: ").size(HeadlineSize::XSmall),
                                )
                                .child(
                                    Headline::new(&next_option_entries.option_name)
                                        .size(HeadlineSize::XSmall),
                                ),
                        ),
                )
                .child(ListSeparator)
                .children(
                    next_option_entries
                        .navigable_options
                        .iter()
                        .map(|(option, entry)| {
                            div()
                                .id(format!("li-parent-{}", option))
                                .track_focus(&entry.focus_handle)
                                .on_action({
                                    let mut template = template_entry.clone();
                                    template.options_selected.insert(
                                        next_option_entries.option_name.clone(),
                                        option.clone(),
                                    );
                                    cx.listener(move |this, _: &menu::Confirm, window, cx| {
                                        this.accept_message(
                                            DevContainerMessage::TemplateOptionsSpecified(
                                                template.clone(),
                                            ),
                                            window,
                                            cx,
                                        );
                                    })
                                })
                                .child(
                                    ListItem::new(format!("li-option-{}", option))
                                        .inset(true)
                                        .spacing(ui::ListItemSpacing::Sparse)
                                        .toggle_state(
                                            entry.focus_handle.contains_focused(window, cx),
                                        )
                                        .on_click({
                                            let mut template = template_entry.clone();
                                            template.options_selected.insert(
                                                next_option_entries.option_name.clone(),
                                                option.clone(),
                                            );
                                            cx.listener(move |this, _, window, cx| {
                                                this.accept_message(
                                                    DevContainerMessage::TemplateOptionsSpecified(
                                                        template.clone(),
                                                    ),
                                                    window,
                                                    cx,
                                                );
                                                cx.notify();
                                            })
                                        })
                                        .child(Label::new(option)),
                                )
                        }),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::GoBack, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Return).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(DevContainerMessage::GoBack, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Go Back")),
                        ),
                )
                .into_any_element(),
        );
        for (_, entry) in &next_option_entries.navigable_options {
            view = view.entry(entry.clone());
        }
        view = view.entry(self.back_entry.clone());
        view.render(window, cx).into_any_element()
    }

    fn render_features_query_returned(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(picker) = &self.features_picker {
            let picker_element = div()
                .track_focus(&self.focus_handle(cx))
                .child(picker.clone().into_any_element())
                .into_any_element();
            picker.focus_handle(cx).focus(window, cx);
            picker_element
        } else {
            div().into_any_element()
        }
    }

    fn render_confirming_write_dev_container(
        &self,
        template_entry: TemplateEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Navigable::new(
            div()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new()
                            .icon(Icon::new(IconName::Warning).color(Color::Warning))
                            .child(
                                Headline::new("Overwrite Existing Configuration?")
                                    .size(HeadlineSize::XSmall),
                            ),
                    ),
                )
                .child(
                    div()
                        .track_focus(&self.confirm_entry.focus_handle)
                        .on_action({
                            let template = template_entry.clone();
                            cx.listener(move |this, _: &menu::Confirm, window, cx| {
                                this.accept_message(
                                    DevContainerMessage::ConfirmWriteDevContainer(template.clone()),
                                    window,
                                    cx,
                                );
                            })
                        })
                        .child(
                            ListItem::new("li-search-containers")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Check).color(Color::Muted))
                                .toggle_state(
                                    self.confirm_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.accept_message(
                                        DevContainerMessage::ConfirmWriteDevContainer(
                                            template_entry.clone(),
                                        ),
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                }))
                                .child(Label::new("Overwrite")),
                        ),
                )
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.dismiss(&menu::Cancel, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::XCircle).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.dismiss(&menu::Cancel, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Cancel")),
                        ),
                )
                .into_any_element(),
        )
        .entry(self.confirm_entry.clone())
        .entry(self.back_entry.clone())
        .render(window, cx)
        .into_any_element()
    }

    fn render_querying_templates(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        Navigable::new(
            div()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new().child(
                            Headline::new("Create Dev Container").size(HeadlineSize::XSmall),
                        ),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div().child(
                        ListItem::new("li-querying")
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(
                                Icon::new(IconName::ArrowCircle)
                                    .color(Color::Muted)
                                    .with_rotate_animation(2),
                            )
                            .child(Label::new("Querying template registry...")),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::GoBack, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Pencil).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(DevContainerMessage::GoBack, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Go Back")),
                        ),
                )
                .into_any_element(),
        )
        .entry(self.back_entry.clone())
        .render(window, cx)
        .into_any_element()
    }
    fn render_querying_features(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        Navigable::new(
            div()
                .child(
                    div().track_focus(&self.focus_handle).child(
                        ModalHeader::new().child(
                            Headline::new("Create Dev Container").size(HeadlineSize::XSmall),
                        ),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div().child(
                        ListItem::new("li-querying")
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(
                                Icon::new(IconName::ArrowCircle)
                                    .color(Color::Muted)
                                    .with_rotate_animation(2),
                            )
                            .child(Label::new("Querying features...")),
                    ),
                )
                .child(ListSeparator)
                .child(
                    div()
                        .track_focus(&self.back_entry.focus_handle)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.accept_message(DevContainerMessage::GoBack, window, cx);
                        }))
                        .child(
                            ListItem::new("li-goback")
                                .inset(true)
                                .spacing(ui::ListItemSpacing::Sparse)
                                .start_slot(Icon::new(IconName::Pencil).color(Color::Muted))
                                .toggle_state(
                                    self.back_entry.focus_handle.contains_focused(window, cx),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_message(DevContainerMessage::GoBack, window, cx);
                                    cx.notify();
                                }))
                                .child(Label::new("Go Back")),
                        ),
                )
                .into_any_element(),
        )
        .entry(self.back_entry.clone())
        .render(window, cx)
        .into_any_element()
    }
}

impl StatefulModal for DevContainerModal {
    type State = DevContainerState;
    type Message = DevContainerMessage;

    fn state(&self) -> Self::State {
        self.state.clone()
    }

    fn render_for_state(
        &self,
        state: Self::State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match state {
            DevContainerState::Initial => self.render_initial(window, cx),
            DevContainerState::QueryingTemplates => self.render_querying_templates(window, cx),
            DevContainerState::TemplateQueryReturned(Ok(_)) => {
                self.render_retrieved_templates(window, cx)
            }
            DevContainerState::UserOptionsSpecifying(template_entry) => {
                self.render_user_options_specifying(template_entry, window, cx)
            }
            DevContainerState::QueryingFeatures(_) => self.render_querying_features(window, cx),
            DevContainerState::FeaturesQueryReturned(_) => {
                self.render_features_query_returned(window, cx)
            }
            DevContainerState::ConfirmingWriteDevContainer(template_entry) => {
                self.render_confirming_write_dev_container(template_entry, window, cx)
            }
            DevContainerState::TemplateWriteFailed(dev_container_error) => self.render_error(
                "Error Creating Dev Container Definition".to_string(),
                dev_container_error,
                window,
                cx,
            ),
            DevContainerState::TemplateQueryReturned(Err(e)) => {
                self.render_error("Error Retrieving Templates".to_string(), e, window, cx)
            }
        }
    }

    fn accept_message(
        &mut self,
        message: Self::Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_state = match message {
            DevContainerMessage::SearchTemplates => {
                cx.spawn_in(window, async move |this, cx| {
                    let Ok(client) = cx.update(|_, cx| cx.http_client()) else {
                        return;
                    };
                    match get_ghcr_templates(client).await {
                        Ok(templates) => {
                            let message =
                                DevContainerMessage::TemplatesRetrieved(templates.templates);
                            this.update_in(cx, |this, window, cx| {
                                this.accept_message(message, window, cx);
                            })
                            .ok();
                        }
                        Err(e) => {
                            let message = DevContainerMessage::ErrorRetrievingTemplates(e);
                            this.update_in(cx, |this, window, cx| {
                                this.accept_message(message, window, cx);
                            })
                            .ok();
                        }
                    }
                })
                .detach();
                Some(DevContainerState::QueryingTemplates)
            }
            DevContainerMessage::ErrorRetrievingTemplates(message) => {
                Some(DevContainerState::TemplateQueryReturned(Err(message)))
            }
            DevContainerMessage::GoBack => match &self.state {
                DevContainerState::Initial => Some(DevContainerState::Initial),
                DevContainerState::QueryingTemplates => Some(DevContainerState::Initial),
                DevContainerState::UserOptionsSpecifying(template_entry) => {
                    if template_entry.current_option_index <= 1 {
                        self.accept_message(DevContainerMessage::SearchTemplates, window, cx);
                    } else {
                        let mut template_entry = template_entry.clone();
                        template_entry.current_option_index =
                            template_entry.current_option_index.saturating_sub(2);
                        self.accept_message(
                            DevContainerMessage::TemplateOptionsSpecified(template_entry),
                            window,
                            cx,
                        );
                    }
                    None
                }
                _ => Some(DevContainerState::Initial),
            },
            DevContainerMessage::TemplatesRetrieved(items) => {
                let items = items
                    .into_iter()
                    .map(|item| TemplateEntry {
                        template: item,
                        options_selected: HashMap::new(),
                        current_option_index: 0,
                        current_option: None,
                        features_selected: HashSet::new(),
                    })
                    .collect::<Vec<TemplateEntry>>();
                if self.state == DevContainerState::QueryingTemplates {
                    let delegate = TemplatePickerDelegate::new(
                        "Select a template".to_string(),
                        cx.weak_entity(),
                        items.clone(),
                        Box::new(|entry, this, window, cx| {
                            this.accept_message(
                                DevContainerMessage::TemplateSelected(entry),
                                window,
                                cx,
                            );
                        }),
                    );

                    let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).embedded());
                    self.picker = Some(picker);
                    Some(DevContainerState::TemplateQueryReturned(Ok(items)))
                } else {
                    None
                }
            }
            DevContainerMessage::TemplateSelected(mut template_entry) => {
                let Some(options) = template_entry.template.clone().options else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let options = options
                    .iter()
                    .collect::<Vec<(&String, &TemplateOptions)>>()
                    .clone();

                let Some((first_option_name, first_option)) =
                    options.get(template_entry.current_option_index)
                else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let next_option_entries = first_option
                    .possible_values()
                    .into_iter()
                    .map(|option| (option, NavigableEntry::focusable(cx)))
                    .collect();

                template_entry.current_option_index += 1;
                template_entry.current_option = Some(TemplateOptionSelection {
                    option_name: (*first_option_name).clone(),
                    description: first_option
                        .description
                        .clone()
                        .unwrap_or_else(|| "".to_string()),
                    navigable_options: next_option_entries,
                });

                Some(DevContainerState::UserOptionsSpecifying(template_entry))
            }
            DevContainerMessage::TemplateOptionsSpecified(mut template_entry) => {
                let Some(options) = template_entry.template.clone().options else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let options = options
                    .iter()
                    .collect::<Vec<(&String, &TemplateOptions)>>()
                    .clone();

                let Some((next_option_name, next_option)) =
                    options.get(template_entry.current_option_index)
                else {
                    return self.accept_message(
                        DevContainerMessage::TemplateOptionsCompleted(template_entry),
                        window,
                        cx,
                    );
                };

                let next_option_entries = next_option
                    .possible_values()
                    .into_iter()
                    .map(|option| (option, NavigableEntry::focusable(cx)))
                    .collect();

                template_entry.current_option_index += 1;
                template_entry.current_option = Some(TemplateOptionSelection {
                    option_name: (*next_option_name).clone(),
                    description: next_option
                        .description
                        .clone()
                        .unwrap_or_else(|| "".to_string()),
                    navigable_options: next_option_entries,
                });

                Some(DevContainerState::UserOptionsSpecifying(template_entry))
            }
            DevContainerMessage::TemplateOptionsCompleted(template_entry) => {
                cx.spawn_in(window, async move |this, cx| {
                    let Ok(client) = cx.update(|_, cx| cx.http_client()) else {
                        return;
                    };
                    let Some(features) = get_ghcr_features(client).await.log_err() else {
                        return;
                    };
                    let message = DevContainerMessage::FeaturesRetrieved(features.features);
                    this.update_in(cx, |this, window, cx| {
                        this.accept_message(message, window, cx);
                    })
                    .ok();
                })
                .detach();
                Some(DevContainerState::QueryingFeatures(template_entry))
            }
            DevContainerMessage::FeaturesRetrieved(features) => {
                if let DevContainerState::QueryingFeatures(template_entry) = self.state.clone() {
                    let features = features
                        .iter()
                        .map(|feature| FeatureEntry {
                            feature: feature.clone(),
                            toggle_state: ToggleState::Unselected,
                        })
                        .collect::<Vec<FeatureEntry>>();
                    let delegate = FeaturePickerDelegate::new(
                        "Select features to add".to_string(),
                        cx.weak_entity(),
                        features,
                        template_entry.clone(),
                        Box::new(|entry, this, window, cx| {
                            this.accept_message(
                                DevContainerMessage::FeaturesSelected(entry),
                                window,
                                cx,
                            );
                        }),
                    );

                    let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).embedded());
                    self.features_picker = Some(picker);
                    Some(DevContainerState::FeaturesQueryReturned(template_entry))
                } else {
                    None
                }
            }
            DevContainerMessage::FeaturesSelected(template_entry) => {
                if let Some(workspace) = self.workspace.upgrade() {
                    dispatch_apply_templates(template_entry, workspace, window, true, cx);
                }

                None
            }
            DevContainerMessage::NeedConfirmWriteDevContainer(template_entry) => Some(
                DevContainerState::ConfirmingWriteDevContainer(template_entry),
            ),
            DevContainerMessage::ConfirmWriteDevContainer(template_entry) => {
                if let Some(workspace) = self.workspace.upgrade() {
                    dispatch_apply_templates(template_entry, workspace, window, false, cx);
                }
                None
            }
            DevContainerMessage::FailedToWriteTemplate(error) => {
                Some(DevContainerState::TemplateWriteFailed(error))
            }
        };
        if let Some(state) = new_state {
            self.state = state;
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }
}
impl EventEmitter<DismissEvent> for DevContainerModal {}
impl Focusable for DevContainerModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
impl ModalView for DevContainerModal {}

impl Render for DevContainerModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_inner(window, cx)
    }
}

trait StatefulModal: ModalView + EventEmitter<DismissEvent> + Render {
    type State;
    type Message;

    fn state(&self) -> Self::State;

    fn render_for_state(
        &self,
        state: Self::State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement;

    fn accept_message(
        &mut self,
        message: Self::Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    );

    fn dismiss(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn render_inner(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let element = self.render_for_state(self.state(), window, cx);
        div()
            .elevation_3(cx)
            .w(rems(34.))
            .key_context("ContainerModal")
            .on_action(cx.listener(Self::dismiss))
            .child(element)
    }
}

fn ghcr_registry() -> &'static str {
    "ghcr.io"
}

fn devcontainer_templates_repository() -> &'static str {
    "devcontainers/templates"
}

fn devcontainer_features_repository() -> &'static str {
    "devcontainers/features"
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TemplateOptions {
    #[serde(rename = "type")]
    option_type: String,
    description: Option<String>,
    proposals: Option<Vec<String>>,
    #[serde(rename = "enum")]
    enum_values: Option<Vec<String>>,
    // Different repositories surface "default: 'true'" or "default: true",
    // so we need to be flexible in deserializing
    #[serde(deserialize_with = "deserialize_string_or_bool")]
    default: String,
}

fn deserialize_string_or_bool<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrBool {
        String(String),
        Bool(bool),
    }

    match StringOrBool::deserialize(deserializer)? {
        StringOrBool::String(s) => Ok(s),
        StringOrBool::Bool(b) => Ok(b.to_string()),
    }
}

impl TemplateOptions {
    fn possible_values(&self) -> Vec<String> {
        match self.option_type.as_str() {
            "string" => self
                .enum_values
                .clone()
                .or(self.proposals.clone().or(Some(vec![self.default.clone()])))
                .unwrap_or_default(),
            // If not string, must be boolean
            _ => {
                if self.default == "true" {
                    vec!["true".to_string(), "false".to_string()]
                } else {
                    vec!["false".to_string(), "true".to_string()]
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
struct DevContainerFeature {
    id: String,
    version: String,
    name: String,
    source_repository: Option<String>,
}

impl DevContainerFeature {
    fn major_version(&self) -> String {
        let Some(mv) = self.version.get(..1) else {
            return "".to_string();
        };
        mv.to_string()
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DevContainerTemplate {
    id: String,
    name: String,
    options: Option<HashMap<String, TemplateOptions>>,
    source_repository: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevContainerFeaturesResponse {
    features: Vec<DevContainerFeature>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevContainerTemplatesResponse {
    templates: Vec<DevContainerTemplate>,
}

fn dispatch_apply_templates(
    template_entry: TemplateEntry,
    workspace: Entity<Workspace>,
    window: &mut Window,
    check_for_existing: bool,
    cx: &mut Context<DevContainerModal>,
) {
    cx.spawn_in(window, async move |this, cx| {
        let Some((tree_id, context)) = workspace.update(cx, |workspace, cx| {
            let worktree = workspace
                .project()
                .read(cx)
                .visible_worktrees(cx)
                .find_map(|tree| {
                    tree.read(cx)
                        .root_entry()?
                        .is_dir()
                        .then_some(tree.read(cx))
                });
            let tree_id = worktree.map(|w| w.id())?;
            let context = DevContainerContext::from_workspace(workspace, cx)?;
            Some((tree_id, context))
        }) else {
            return;
        };

        let environment = context.environment(cx).await;

        {
            if check_for_existing
                && read_default_devcontainer_configuration(&context, environment)
                    .await
                    .is_ok()
            {
                this.update_in(cx, |this, window, cx| {
                    this.accept_message(
                        DevContainerMessage::NeedConfirmWriteDevContainer(template_entry),
                        window,
                        cx,
                    );
                })
                .ok();
                return;
            }

            let worktree = workspace.read_with(cx, |workspace, cx| {
                workspace.project().read(cx).worktree_for_id(tree_id, cx)
            });

            let files = match apply_devcontainer_template(
                worktree.unwrap(),
                &template_entry.template,
                &template_entry.options_selected,
                &template_entry.features_selected,
                &context,
                cx,
            )
            .await
            {
                Ok(files) => files,
                Err(e) => {
                    this.update_in(cx, |this, window, cx| {
                        this.accept_message(
                            DevContainerMessage::FailedToWriteTemplate(
                                DevContainerError::DevContainerTemplateApplyFailed(e.to_string()),
                            ),
                            window,
                            cx,
                        );
                    })
                    .ok();
                    return;
                }
            };

            if files.project_files.contains(&Arc::from(
                RelPath::from_unix_str(".devcontainer/devcontainer.json").unwrap(),
            )) {
                let Some(workspace_task) = workspace
                    .update_in(cx, |workspace, window, cx| {
                        let Ok(path) = RelPath::from_unix_str(".devcontainer/devcontainer.json")
                        else {
                            return Task::ready(Err(anyhow!(
                                "Couldn't create path for .devcontainer/devcontainer.json"
                            )));
                        };
                        workspace.open_path((tree_id, path), None, true, window, cx)
                    })
                    .ok()
                else {
                    return;
                };

                workspace_task.await.log_err();
            }
            this.update_in(cx, |this, window, cx| {
                this.dismiss(&menu::Cancel, window, cx);
            })
            .ok();
        }
    })
    .detach();
}

async fn get_ghcr_templates(
    client: Arc<dyn HttpClient>,
) -> Result<DevContainerTemplatesResponse, String> {
    let token = get_oci_token(
        ghcr_registry(),
        devcontainer_templates_repository(),
        &client,
    )
    .await?;
    let manifest = get_latest_oci_manifest(
        &token.token,
        ghcr_registry(),
        devcontainer_templates_repository(),
        &client,
        None,
    )
    .await?;

    let mut template_response: DevContainerTemplatesResponse = get_deserializable_oci_blob(
        &token.token,
        ghcr_registry(),
        devcontainer_templates_repository(),
        &manifest.layers[0].digest,
        &client,
    )
    .await?;

    for template in &mut template_response.templates {
        template.source_repository = Some(format!(
            "{}/{}",
            ghcr_registry(),
            devcontainer_templates_repository()
        ));
    }
    Ok(template_response)
}

async fn get_ghcr_features(
    client: Arc<dyn HttpClient>,
) -> Result<DevContainerFeaturesResponse, String> {
    let token = get_oci_token(
        ghcr_registry(),
        devcontainer_templates_repository(),
        &client,
    )
    .await?;

    let manifest = get_latest_oci_manifest(
        &token.token,
        ghcr_registry(),
        devcontainer_features_repository(),
        &client,
        None,
    )
    .await?;

    let mut features_response: DevContainerFeaturesResponse = get_deserializable_oci_blob(
        &token.token,
        ghcr_registry(),
        devcontainer_features_repository(),
        &manifest.layers[0].digest,
        &client,
    )
    .await?;

    for feature in &mut features_response.features {
        feature.source_repository = Some(format!(
            "{}/{}",
            ghcr_registry(),
            devcontainer_features_repository()
        ));
    }
    Ok(features_response)
}

#[cfg(test)]
mod tests {
    use http_client::{FakeHttpClient, anyhow};

    use crate::{
        DevContainerTemplatesResponse, devcontainer_templates_repository,
        get_deserializable_oci_blob, ghcr_registry,
    };

    #[gpui::test]
    async fn test_get_devcontainer_templates() {
        let client = FakeHttpClient::create(|request| async move {
            let host = request.uri().host();
            if host.is_none() || host.unwrap() != "ghcr.io" {
                return Err(anyhow!("Unexpected host: {}", host.unwrap_or_default()));
            }
            let path = request.uri().path();
            if path
                != format!(
                    "/v2/{}/blobs/sha256:035e9c9fd9bd61f6d3965fa4bf11f3ddfd2490a8cf324f152c13cc3724d67d09",
                    devcontainer_templates_repository()
                )
            {
                return Err(anyhow!("Unexpected path: {}", path));
            }
            Ok(http_client::Response::builder()
                .status(200)
                .body("{
                    \"sourceInformation\": {
                        \"source\": \"devcontainer-cli\"
                    },
                    \"templates\": [
                        {
                            \"id\": \"alpine\",
                            \"version\": \"3.4.0\",
                            \"name\": \"Alpine\",
                            \"description\": \"Simple Alpine container with Git installed.\",
                            \"documentationURL\": \"https://github.com/devcontainers/templates/tree/main/src/alpine\",
                            \"publisher\": \"Dev Container Spec Maintainers\",
                            \"licenseURL\": \"https://github.com/devcontainers/templates/blob/main/LICENSE\",
                            \"options\": {
                                \"imageVariant\": {
                                    \"type\": \"string\",
                                    \"description\": \"Alpine version:\",
                                    \"proposals\": [
                                        \"3.21\",
                                        \"3.20\",
                                        \"3.19\",
                                        \"3.18\"
                                    ],
                                    \"default\": \"3.20\"
                                }
                            },
                            \"platforms\": [
                                \"Any\"
                            ],
                            \"optionalPaths\": [
                                \".github/dependabot.yml\"
                            ],
                            \"type\": \"image\",
                            \"files\": [
                                \"NOTES.md\",
                                \"README.md\",
                                \"devcontainer-template.json\",
                                \".devcontainer/devcontainer.json\",
                                \".github/dependabot.yml\"
                            ],
                            \"fileCount\": 5,
                            \"featureIds\": []
                        }
                    ]
                }".into())
                .unwrap())
        });
        let response: Result<DevContainerTemplatesResponse, String> = get_deserializable_oci_blob(
            "",
            ghcr_registry(),
            devcontainer_templates_repository(),
            "sha256:035e9c9fd9bd61f6d3965fa4bf11f3ddfd2490a8cf324f152c13cc3724d67d09",
            &client,
        )
        .await;
        assert!(response.is_ok());
        let response = response.unwrap();
        assert_eq!(response.templates.len(), 1);
        assert_eq!(response.templates[0].name, "Alpine");
    }

    #[test]
    fn session_cache_drops_environments_of_restarted_or_removed_containers() {
        let cache = crate::SessionCache::default();
        let key =
            |container_id: &str, started_at: &str, remote_user: &str| crate::UserEnvironmentKey {
                container_id: container_id.to_string(),
                started_at: Some(started_at.to_string()),
                remote_user: remote_user.to_string(),
            };
        let environment = std::collections::HashMap::from([("A".to_string(), "1".to_string())]);

        cache.set_user_environment(key("first", "t1", "root"), environment.clone());
        cache.set_user_environment(key("first", "t1", "vscode"), environment.clone());
        cache.set_user_environment(key("second", "t1", "root"), environment.clone());
        cache.set_user_environment(key("first", "t2", "root"), environment);
        assert!(
            cache
                .user_environment(&key("first", "t1", "root"))
                .is_none()
        );
        assert!(
            cache
                .user_environment(&key("first", "t1", "vscode"))
                .is_none()
        );
        assert!(
            cache
                .user_environment(&key("first", "t2", "root"))
                .is_some()
        );
        assert!(
            cache
                .user_environment(&key("second", "t1", "root"))
                .is_some()
        );

        cache.forget_container("first");
        assert!(
            cache
                .user_environment(&key("first", "t2", "root"))
                .is_none()
        );
        assert!(
            cache
                .user_environment(&key("second", "t1", "root"))
                .is_some()
        );
    }

    #[test]
    fn each_dev_container_has_its_own_log() {
        let first = super::dev_container_log_path(
            "/home/me/app",
            "/home/me/app/.devcontainer/devcontainer.json",
        );
        assert_eq!(
            first,
            super::dev_container_log_path(
                "/home/me/app",
                "/home/me/app/.devcontainer/devcontainer.json"
            )
        );
        assert_ne!(
            first,
            super::dev_container_log_path(
                "/home/me/api",
                "/home/me/api/.devcontainer/devcontainer.json"
            )
        );
        assert_ne!(
            first,
            super::dev_container_log_path(
                "/home/me/app",
                "/home/me/app/.devcontainer/web/devcontainer.json"
            )
        );
    }
}
