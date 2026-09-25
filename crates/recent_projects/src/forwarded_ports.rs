use std::sync::Arc;

use gpui::{
    Action as _, AnyElement, App, Context, DismissEvent, Global, SharedString, Task, Window,
};
use picker::{Picker, PickerDelegate};
use remote::{ForwardedPort, RemoteConnectionOptions};
use ui::{
    Color, Icon, IconName, Label, LabelCommon as _, LabelSize, ListItem, ListItemSpacing,
    prelude::*,
};
use workspace::Workspace;
use zed_actions::ShowForwardedPorts;

/// The ports forwarded from dev containers to this machine.
#[derive(Default)]
struct ForwardedPorts(Vec<ForwardedPort>);

impl Global for ForwardedPorts {}

pub(crate) fn init(cx: &mut App) {
    cx.set_global(ForwardedPorts::default());
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(show_forwarded_ports);
    })
    .detach();
}

pub(crate) fn insert(forwarded: ForwardedPort, cx: &mut App) {
    if forwarded.local_port.is_none() {
        return;
    }
    let ports = &mut cx.global_mut::<ForwardedPorts>().0;
    ports.retain(|port| {
        !(port.container_id == forwarded.container_id && port.port == forwarded.port)
    });
    ports.push(forwarded);
    ports.sort_by_key(|port| port.port);
}

pub(crate) fn remove(container_id: &str, port: u16, cx: &mut App) {
    cx.global_mut::<ForwardedPorts>()
        .0
        .retain(|forwarded| !(forwarded.container_id == container_id && forwarded.port == port));
}

/// The address that reaches `forwarded` on this machine.
pub(crate) fn url(forwarded: &ForwardedPort, local_port: u16) -> String {
    let scheme = if forwarded.https { "https" } else { "http" };
    format!("{scheme}://localhost:{local_port}")
}

fn show_forwarded_ports(
    workspace: &mut Workspace,
    _: &ShowForwardedPorts,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(RemoteConnectionOptions::Docker(options)) =
        workspace.project().read(cx).remote_connection_options(cx)
    else {
        cx.propagate();
        return;
    };
    let container_id = options.container_id;
    workspace.toggle_modal(window, cx, |window, cx| {
        let delegate = ForwardedPortsDelegate::new(container_id, cx);
        Picker::uniform_list(delegate, window, cx)
    });
}

enum Entry {
    Forwarded(ForwardedPort),
    /// Forwards the port typed in the query.
    Forward(u16),
}

struct ForwardedPortsDelegate {
    container_id: String,
    entries: Vec<Entry>,
    selected_index: usize,
}

impl ForwardedPortsDelegate {
    fn new(container_id: String, cx: &App) -> Self {
        let mut this = Self {
            container_id,
            entries: Vec::new(),
            selected_index: 0,
        };
        this.entries = this.matching_entries("", cx);
        this
    }

    fn matching_entries(&self, query: &str, cx: &App) -> Vec<Entry> {
        let query = query.trim();
        let forwarded =
            cx.global::<ForwardedPorts>()
                .0
                .iter()
                .filter(|forwarded| forwarded.container_id == self.container_id)
                .filter(|forwarded| {
                    query.is_empty()
                        || forwarded.port.to_string().contains(query)
                        || forwarded.label.as_ref().is_some_and(|label| {
                            label.to_lowercase().contains(&query.to_lowercase())
                        })
                });
        let mut entries: Vec<Entry> = forwarded.cloned().map(Entry::Forwarded).collect();
        if let Ok(port) = query.parse::<u16>()
            && port != 0
            && !entries
                .iter()
                .any(|entry| matches!(entry, Entry::Forwarded(forwarded) if forwarded.port == port))
        {
            entries.push(Entry::Forward(port));
        }
        entries
    }
}

impl PickerDelegate for ForwardedPortsDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "forwarded ports"
    }

    fn match_count(&self) -> usize {
        self.entries.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, _: &mut Context<Picker<Self>>) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Type a port number to forward it".into()
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        Some("No ports are forwarded. Type a port number to forward it.".into())
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.entries = self.matching_entries(&query, cx);
        self.selected_index = self
            .selected_index
            .min(self.entries.len().saturating_sub(1));
        Task::ready(())
    }

    /// Opens the selected port in the browser, or with `secondary` stops forwarding
    /// it.
    fn confirm(&mut self, secondary: bool, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        match self.entries.get(self.selected_index) {
            Some(Entry::Forwarded(forwarded)) => {
                if secondary {
                    remote::stop_forwarding_container_port(&self.container_id, forwarded.port);
                } else if let Some(local_port) = forwarded.local_port {
                    cx.open_url(&url(forwarded, local_port));
                }
            }
            Some(Entry::Forward(port)) => {
                remote::forward_container_port(&self.container_id, *port);
            }
            None => return,
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
        let (icon, title, detail) = match self.entries.get(ix)? {
            Entry::Forwarded(forwarded) => (
                IconName::Server,
                match &forwarded.label {
                    Some(label) => format!("{} ({label})", forwarded.port),
                    None => forwarded.port.to_string(),
                },
                forwarded
                    .local_port
                    .map(|local_port| url(forwarded, local_port))
                    .unwrap_or_default(),
            ),
            Entry::Forward(port) => (
                IconName::Plus,
                format!("Forward Port {port}"),
                String::new(),
            ),
        };
        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(Icon::new(icon).color(Color::Muted))
                .child(
                    h_flex().gap_2().child(Label::new(title)).child(
                        Label::new(detail)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                )
                .into_any_element(),
        )
    }

    fn render_footer(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .gap_1()
                .justify_end()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(
                    Button::new("open-forwarded-port", "Open in Browser")
                        .key_binding(
                            ui::KeyBinding::for_action(&menu::Confirm, cx)
                                .map(|binding| binding.size(rems_from_px(12_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                        }),
                )
                .child(
                    Button::new("stop-forwarding-port", "Stop Forwarding")
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
    use gpui::TestAppContext;
    use remote::{ForwardNotice, ForwardedPort};

    use super::{Entry, ForwardedPorts, ForwardedPortsDelegate};

    fn forwarded(container_id: &str, port: u16, label: Option<&str>) -> ForwardedPort {
        ForwardedPort {
            container_id: container_id.to_string(),
            port,
            local_port: Some(port),
            label: label.map(str::to_string),
            notice: ForwardNotice::Notify,
            https: false,
        }
    }

    fn ports(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Forwarded(forwarded) => forwarded.port.to_string(),
                Entry::Forward(port) => format!("forward {port}"),
            })
            .collect()
    }

    #[gpui::test]
    fn lists_the_ports_of_the_container_and_offers_to_forward_others(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(ForwardedPorts::default());
            super::insert(forwarded("app", 5432, Some("Postgres")), cx);
            super::insert(forwarded("app", 3000, Some("Web")), cx);
            super::insert(forwarded("other", 8080, None), cx);
            // A port that couldn't be forwarded isn't listed.
            super::insert(
                ForwardedPort {
                    local_port: None,
                    ..forwarded("app", 443, None)
                },
                cx,
            );

            let delegate = ForwardedPortsDelegate::new("app".to_string(), cx);
            assert_eq!(ports(&delegate.entries), ["3000", "5432"]);
            assert_eq!(ports(&delegate.matching_entries("post", cx)), ["5432"]);
            assert_eq!(
                ports(&delegate.matching_entries("8080", cx)),
                ["forward 8080"]
            );
            assert_eq!(ports(&delegate.matching_entries("3000", cx)), ["3000"]);

            super::remove("app", 3000, cx);
            assert_eq!(ports(&delegate.matching_entries("", cx)), ["5432"]);
        });
    }

    #[test]
    fn urls_follow_the_protocol() {
        let mut port = forwarded("app", 8443, None);
        assert_eq!(super::url(&port, 9443), "http://localhost:9443");
        port.https = true;
        assert_eq!(super::url(&port, 9443), "https://localhost:9443");
    }
}
