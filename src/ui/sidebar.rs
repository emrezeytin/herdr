mod tokens;

use ratatui::{
    layout::{Alignment, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use self::tokens::{ResolvedToken, ResolvedTokenKind};
use super::scrollbar::{render_scrollbar, should_show_scrollbar};
use super::status::{state_icon, state_label};
use super::text::{display_width, display_width_u16, truncate_end};
use crate::app::state::{AgentPanelSort, Palette};
use crate::app::{AppState, Mode};
use crate::detect::AgentState;
use crate::terminal::TerminalRuntimeRegistry;

const WORKSPACE_SECTION_HEADER_ROWS: u16 = 2;
const AGENT_PANEL_HEADER_ROWS: u16 = 3;

pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub primary_label: String,
    pub primary_tab_label: Option<String>,
    pub pane_label: Option<String>,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub agent_label: Option<String>,
    pub agent_kind_label: Option<String>,
    pub agent: Option<crate::detect::Agent>,
    pub state: AgentState,
    pub seen: bool,
    pub last_agent_state_change_seq: Option<u64>,
    pub state_labels: std::collections::HashMap<String, String>,
    pub tokens: std::collections::HashMap<String, String>,
}

// sessionr: the sidebar is a single session list. The old workspaces/agents
// split is gone; the detail area is always empty and the divider never exists.
pub(crate) fn expanded_sidebar_sections(area: Rect, _split_ratio: f32) -> (Rect, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), Rect::default());
    }

    (content, Rect::default())
}

pub(crate) fn sidebar_section_divider_rect(_area: Rect, _split_ratio: f32) -> Rect {
    Rect::default()
}

fn agent_panel_sort_label(sort: AgentPanelSort) -> &'static str {
    match sort {
        AgentPanelSort::Spaces => "grouped",
        AgentPanelSort::Priority => "priority",
    }
}

pub(crate) fn agent_panel_toggle_rect(area: Rect, sort: AgentPanelSort) -> Rect {
    agent_panel_header_label_rect(area, agent_panel_sort_label(sort))
}

fn agent_panel_header_label_rect(area: Rect, label: &str) -> Rect {
    if area.width == 0 || area.height < 2 {
        return Rect::default();
    }

    let width = display_width_u16(label).min(area.width);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y + 1,
        width,
        1,
    )
}
pub(crate) fn agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, None)
}

pub(crate) fn all_agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    collect_agent_panel_entries_with_runtimes(app, None)
}

pub(crate) fn agent_panel_entries_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, Some(terminal_runtimes))
}

fn agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
) -> Vec<AgentPanelEntry> {
    let mut entries = collect_agent_panel_entries_with_runtimes(app, terminal_runtimes);
    crate::app::agent_view::apply_agent_view(app, &mut entries);
    entries
}

fn collect_agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
) -> Vec<AgentPanelEntry> {
    let empty_runtimes;
    let terminal_runtimes = match terminal_runtimes {
        Some(terminal_runtimes) => terminal_runtimes,
        None => {
            empty_runtimes = TerminalRuntimeRegistry::new();
            &empty_runtimes
        }
    };

    app.workspaces
        .iter()
        .enumerate()
        .flat_map(|(ws_idx, ws)| {
            let multi_tab = ws.tabs.len() > 1;
            let workspace_label = ws.display_name_from(&app.terminals, terminal_runtimes);
            ws.pane_details(&app.terminals)
                .into_iter()
                .map(move |detail| {
                    let show_tab = multi_tab
                        || ws
                            .tabs
                            .get(detail.tab_idx)
                            .is_some_and(|tab| !tab.is_auto_named());
                    AgentPanelEntry {
                        ws_idx,
                        tab_idx: detail.tab_idx,
                        pane_id: detail.pane_id,
                        primary_label: workspace_label.clone(),
                        primary_tab_label: show_tab.then_some(detail.tab_label),
                        pane_label: detail.pane_label,
                        terminal_title: detail.terminal_title,
                        terminal_title_stripped: detail.terminal_title_stripped,
                        agent_label: Some(detail.agent_label),
                        agent_kind_label: detail.agent_kind_label,
                        agent: detail.agent,
                        state: detail.state,
                        seen: detail.seen,
                        last_agent_state_change_seq: detail.last_agent_state_change_seq,
                        state_labels: detail.state_labels,
                        tokens: detail.tokens,
                    }
                })
        })
        .collect()
}

pub(super) fn agent_panel_status_key(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Working, _) => "working",
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Unknown, _) => "unknown",
    }
}
/// Every session renders as two rows: goal line + branch line.
const SESSION_ROW_HEIGHT: u16 = 2;

fn workspace_row_height_in_body(
    _app: &AppState,
    _workspace: &crate::workspace::Workspace,
    _indented: bool,
    body_height: u16,
) -> u16 {
    SESSION_ROW_HEIGHT.min(body_height)
}

/// Height of the pinned focused-session card: two border rows plus goal
/// row, branch row, and an agent chips row when the session has agents.
fn focused_card_height(app: &AppState) -> u16 {
    let Some(focused) = app.active.or(Some(app.selected)) else {
        return 0;
    };
    let Some(ws) = app.workspaces.get(focused) else {
        return 0;
    };
    if ws.is_settled() {
        return 0;
    }
    let has_agents = app
        .terminals
        .values()
        .any(|terminal| terminal.agent_name.is_some());
    if has_agents { 5 } else { 4 }
}

fn workspace_entry_gap(app: &AppState, entries: &[WorkspaceListEntry], entry_idx: usize) -> u16 {
    if entry_idx + 1 < entries.len() && !next_entry_is_indented_workspace(entries, entry_idx) {
        app.sidebar_spaces.row_gap
    } else {
        0
    }
}pub(crate) fn workspace_parent_group_state(
    app: &AppState,
    ws_idx: usize,
) -> Option<(String, bool)> {
    let space = app.workspaces.get(ws_idx)?.worktree_space()?;
    if space.is_linked_worktree {
        return None;
    }
    let member_count = app
        .workspaces
        .iter()
        .filter(|ws| {
            ws.worktree_space()
                .is_some_and(|member| member.key == space.key)
        })
        .count();
    (member_count >= 2).then(|| {
        (
            space.key.clone(),
            app.collapsed_space_keys.contains(&space.key),
        )
    })
}

pub(crate) fn grouped_child_display_label(
    label: &str,
    branch: Option<&str>,
    has_custom_name: bool,
) -> String {
    if has_custom_name {
        return label.to_string();
    }
    let Some(branch) = branch else {
        return label.to_string();
    };
    branch
        .strip_prefix("worktree/")
        .unwrap_or(branch)
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceListEntry {
    Workspace { ws_idx: usize, indented: bool },
    SettledHeader,
    SettledShowMore,
}

pub(crate) fn next_entry_is_indented_workspace(entries: &[WorkspaceListEntry], idx: usize) -> bool {
    matches!(
        entries.get(idx.saturating_add(1)),
        Some(WorkspaceListEntry::Workspace { indented: true, .. })
    )
}

pub(crate) fn normalized_workspace_scroll(app: &AppState, area: Rect, requested: usize) -> usize {
    let ws_area = workspace_list_rect(area, app.sidebar_section_split);
    let body = workspace_list_body_rect(app, ws_area, false);
    if body.height == 0 {
        return requested;
    }

    if workspace_list_entries(app).is_empty() {
        0
    } else {
        requested.min(workspace_list_bottom_start(app, ws_area))
    }
}

pub(crate) fn workspace_list_entries(app: &AppState) -> Vec<WorkspaceListEntry> {
    workspace_list_entries_inner(app, false)
}

/// Like [`workspace_list_entries`] but always expands worktree groups, ignoring
/// `collapsed_space_keys`. The mobile switcher has no collapse affordance and
/// always shows the full worktree tree.
pub(crate) fn workspace_list_entries_expanded(app: &AppState) -> Vec<WorkspaceListEntry> {
    workspace_list_entries_inner(app, true)
}

fn workspace_list_entries_inner(app: &AppState, force_expanded: bool) -> Vec<WorkspaceListEntry> {
    let _ = force_expanded;
    let settled_preview = app.sidebar_spaces.settled_preview;

    let mut active = Vec::new();
    let mut settled = Vec::new();
    for (ws_idx, ws) in app.workspaces.iter().enumerate() {
        if ws.is_settled() {
            settled.push((ws.settled, ws.last_activity, ws_idx));
        } else {
            active.push((ws.last_activity, ws_idx));
        }
    }
    // Sessions sort by recency desc; ties keep workspace order (stable sort).
    active.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    settled.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    let mut entries = Vec::new();
    entries.extend(
        active
            .into_iter()
            .map(|(_, ws_idx)| WorkspaceListEntry::Workspace {
                ws_idx,
                indented: false,
            }),
    );
    if !settled.is_empty() {
        entries.push(WorkspaceListEntry::SettledHeader);
        let collapsed = !app.settled_expanded && settled.len() > settled_preview;
        let visible = if collapsed {
            settled_preview
        } else {
            settled.len()
        };
        entries.extend(
            settled
                .into_iter()
                .take(visible)
                .map(|(_, _, ws_idx)| WorkspaceListEntry::Workspace {
                    ws_idx,
                    indented: false,
                }),
        );
        if collapsed {
            entries.push(WorkspaceListEntry::SettledShowMore);
        }
    }
    entries
}


pub(crate) fn workspace_list_rect(area: Rect, split_ratio: f32) -> Rect {
    let (ws_area, _) = expanded_sidebar_sections(area, split_ratio);
    ws_area
}

pub(crate) fn workspace_list_body_rect(
    app: &AppState,
    area: Rect,
    has_scrollbar: bool,
) -> Rect {
    if area.width == 0 || area.height <= WORKSPACE_SECTION_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area
        .y
        .saturating_add(WORKSPACE_SECTION_HEADER_ROWS)
        .saturating_add(focused_card_height(app));
    let footer_y = area.y + area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn workspace_list_visible_count(app: &AppState, area: Rect, scroll: usize) -> usize {
    let body = workspace_list_body_rect(app, area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    let entries = workspace_list_entries(app);
    for (entry_idx, entry) in entries.iter().enumerate().skip(scroll) {
        let (row_height, gap) = entry_height_and_gap(app, &entries, entry_idx, entry, body);
        if used_rows.saturating_add(row_height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(row_height);
        visible += 1;
        used_rows = used_rows.saturating_add(gap).min(body.height);
    }
    visible
}

fn workspace_list_bottom_start(app: &AppState, area: Rect) -> usize {
    let body = workspace_list_body_rect(app, area, false);
    let entries = workspace_list_entries(app);
    let mut used_rows = 0u16;
    let mut start = entries.len();
    for (entry_idx, entry) in entries.iter().enumerate().rev() {
        let (row_height, gap) = entry_height_and_gap(app, &entries, entry_idx, entry, body);
        if used_rows.saturating_add(row_height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(row_height);
        start = entry_idx;
        used_rows = used_rows.saturating_add(gap).min(body.height);
    }
    start.min(entries.len().saturating_sub(1))
}

fn entry_height_and_gap(
    app: &AppState,
    entries: &[WorkspaceListEntry],
    entry_idx: usize,
    entry: &WorkspaceListEntry,
    body: Rect,
) -> (u16, u16) {
    match entry {
        WorkspaceListEntry::Workspace { ws_idx, indented } => {
            let Some(ws) = app.workspaces.get(*ws_idx) else {
                return (0, 0);
            };
            (
                workspace_row_height_in_body(app, ws, *indented, body.height),
                workspace_entry_gap(app, entries, entry_idx),
            )
        }
        WorkspaceListEntry::SettledHeader | WorkspaceListEntry::SettledShowMore => (1, 0),
    }
}

pub(crate) fn workspace_list_scroll_metrics(
    app: &AppState,
    area: Rect,
) -> crate::pane::ScrollMetrics {
    let max_scroll = workspace_list_bottom_start(app, area);
    let scroll = app.workspace_scroll.min(max_scroll);
    let viewport_rows = workspace_list_visible_count(app, area, scroll);

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_scroll.saturating_sub(scroll),
        max_offset_from_bottom: max_scroll,
        viewport_rows,
    }
}

pub(crate) fn workspace_list_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    let metrics = workspace_list_scroll_metrics(app, area);
    let body = workspace_list_body_rect(app, area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn agent_panel_body_rect(area: Rect, has_scrollbar: bool) -> Rect {
    if area.width == 0 || area.height <= AGENT_PANEL_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(AGENT_PANEL_HEADER_ROWS);
    let body_height = (area.y + area.height).saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn resolved_agent_rows(app: &AppState, entry: &AgentPanelEntry) -> Vec<Vec<ResolvedToken>> {
    let label = entry
        .state_labels
        .get(agent_panel_status_key(entry.state, entry.seen))
        .map(String::as_str)
        .unwrap_or_else(|| state_label(entry.state, entry.seen));
    tokens::agent_rows(&app.sidebar_agents, entry, label)
}

pub(crate) fn agent_entry_height_in_body(
    app: &AppState,
    entry: &AgentPanelEntry,
    body_height: u16,
) -> u16 {
    (resolved_agent_rows(app, entry)
        .len()
        .max(1)
        .min(u16::MAX as usize) as u16)
        .min(body_height)
}

pub(crate) fn agent_entry_gap(app: &AppState, entry_idx: usize, entry_count: usize) -> u16 {
    if entry_idx + 1 < entry_count {
        app.sidebar_agents.row_gap
    } else {
        0
    }
}

fn agent_panel_visible_count_from(app: &AppState, area: Rect, scroll: usize) -> usize {
    let body = agent_panel_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    let entries = agent_panel_entries(app);
    for (index, entry) in entries.iter().enumerate().skip(scroll) {
        let height = agent_entry_height_in_body(app, entry, body.height);
        if used_rows.saturating_add(height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(height);
        visible += 1;
        used_rows = used_rows
            .saturating_add(agent_entry_gap(app, index, entries.len()))
            .min(body.height);
    }
    visible
}

fn agent_panel_bottom_start(app: &AppState, area: Rect) -> usize {
    let body = agent_panel_body_rect(area, false);
    let entries = agent_panel_entries(app);
    let mut used_rows = 0u16;
    let mut start = entries.len();
    for (index, entry) in entries.iter().enumerate().rev() {
        let gap = agent_entry_gap(app, index, entries.len());
        let needed = agent_entry_height_in_body(app, entry, body.height).saturating_add(gap);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        start = index;
    }
    start.min(entries.len().saturating_sub(1))
}

pub(crate) fn agent_panel_scroll_for_target(
    app: &AppState,
    area: Rect,
    current_scroll: usize,
    target: usize,
) -> usize {
    let max_scroll = agent_panel_bottom_start(app, area);
    if target < current_scroll {
        return target.min(max_scroll);
    }
    let mut scroll = current_scroll.min(max_scroll);
    while scroll < target {
        let visible = agent_panel_visible_count_from(app, area, scroll);
        if visible > 0 && target < scroll.saturating_add(visible) {
            break;
        }
        scroll += 1;
    }
    scroll.min(max_scroll)
}

pub(crate) fn agent_panel_scroll_metrics(app: &AppState, area: Rect) -> crate::pane::ScrollMetrics {
    let max_scroll = agent_panel_bottom_start(app, area);
    let scroll = app.agent_panel_scroll.min(max_scroll);
    let viewport_rows = agent_panel_visible_count_from(app, area, scroll);

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_scroll.saturating_sub(scroll),
        max_offset_from_bottom: max_scroll,
        viewport_rows,
    }
}

pub(crate) fn agent_panel_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    let metrics = agent_panel_scroll_metrics(app, area);
    let body = agent_panel_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn compute_workspace_list_areas(
    app: &AppState,
    area: Rect,
) -> (Vec<crate::app::state::WorkspaceCardArea>, Vec<()>) {
    let ws_area = workspace_list_rect(area, app.sidebar_section_split);
    if ws_area == Rect::default() {
        return (Vec::new(), Vec::new());
    }

    let metrics = workspace_list_scroll_metrics(app, ws_area);
    let body = workspace_list_body_rect(app, ws_area, should_show_scrollbar(metrics));
    if body.width == 0 || body.height == 0 {
        return (Vec::new(), Vec::new());
    }

    let scroll = app.workspace_scroll;
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut cards = Vec::new();
    let headers = Vec::new();

    let entries = workspace_list_entries(app);
    for (entry_idx, entry) in entries.iter().enumerate().skip(scroll) {
        let (row_height, gap) = entry_height_and_gap(app, &entries, entry_idx, entry, body);
        match entry {
            WorkspaceListEntry::Workspace { ws_idx, indented } => {
                if row_y.saturating_add(row_height) > body_bottom {
                    break;
                }
                cards.push(crate::app::state::WorkspaceCardArea {
                    ws_idx: *ws_idx,
                    rect: Rect::new(body.x, row_y, body.width, row_height),
                    indented: *indented,
                });
            }
            WorkspaceListEntry::SettledHeader | WorkspaceListEntry::SettledShowMore => {
                if row_y.saturating_add(row_height) > body_bottom {
                    break;
                }
            }
        }
        row_y = row_y
            .saturating_add(row_height)
            .saturating_add(gap)
            .min(body_bottom);
    }

    (cards, headers)
}

pub(crate) fn compute_workspace_card_areas(
    app: &AppState,
    area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    compute_workspace_list_areas(app, area).0
}

pub(crate) fn workspace_group_chevron_rect(card: &crate::app::state::WorkspaceCardArea) -> Rect {
    if card.rect.width == 0 || card.rect.height == 0 {
        return Rect::default();
    }

    Rect::new(
        card.rect.x + card.rect.width.saturating_sub(1),
        card.rect.y,
        1,
        1,
    )
}

/// Auto-scale sidebar width based on workspace identity + agent summary.
// sessionr: the collapsed sidebar is a single strip of session rows; no
// divider and no agent detail section.
pub(crate) fn collapsed_sidebar_sections(area: Rect) -> (Rect, Option<u16>, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), None, Rect::default());
    }

    (content, None, Rect::default())
}

/// Collapsed sidebar: workspace glance on top, compact agent list below.
pub(super) fn render_sidebar_collapsed(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let is_navigating = matches!(app.mode, Mode::Navigate);

    let p = &app.palette;
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(p.sidebar_bg));
    let sep_style = if is_navigating {
        Style::default().fg(p.accent)
    } else {
        Style::default().fg(p.surface_dim)
    };
    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        render_sidebar_toggle(app, frame, area, true, p);
        return;
    }

    let order = app.visible_workspace_order();
    for (position, ws_idx) in order.iter().enumerate() {
        let y = content.y + position as u16;
        if y >= content.y + content.height {
            break;
        }
        let Some(ws) = app.workspaces.get(*ws_idx) else {
            continue;
        };
        let settled = ws.is_settled();
        let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);
        let (icon, icon_style) = if settled {
            ("·", Style::default().fg(p.overlay0))
        } else {
            state_icon(agg_state, agg_seen, app.status_indicators, p)
        };
        let is_selected = *ws_idx == app.selected && is_navigating;
        let is_active = Some(*ws_idx) == app.active;
        let row_style = if is_selected {
            Style::default().bg(p.selection_bg)
        } else if is_active {
            Style::default().bg(p.active_row_bg)
        } else {
            Style::default()
        };
        let num_style = if settled {
            Style::default().fg(p.overlay0)
        } else if is_selected {
            Style::default().fg(p.overlay1).bg(p.selection_bg)
        } else if is_active {
            Style::default().fg(p.text).bg(p.active_row_bg)
        } else {
            Style::default().fg(p.overlay0)
        };

        if is_selected || is_active {
            let buf = frame.buffer_mut();
            for x in content.x..content.x + content.width {
                buf[(x, y)].set_style(row_style);
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{:<2}", position + 1), num_style),
                Span::styled(icon, icon_style),
            ])),
            Rect::new(content.x, y, content.width, 1),
        );
    }

    render_sidebar_toggle(app, frame, area, true, p);
}

/// Click target for the "+ Show N more" row in the Settled section.
pub(crate) fn settled_show_more_rect(app: &AppState, area: Rect) -> Option<Rect> {
    if area.width == 0 || area.height == 0 || app.settled_expanded {
        return None;
    }
    let metrics = workspace_list_scroll_metrics(app, area);
    let body = workspace_list_body_rect(app, area, should_show_scrollbar(metrics));
    if body == Rect::default() {
        return None;
    }
    let entries = workspace_list_entries(app);
    let scroll = app.workspace_scroll.min(metrics.max_offset_from_bottom);
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    for (entry_idx, entry) in entries.iter().enumerate().skip(scroll) {
        let (row_height, gap) = entry_height_and_gap(app, &entries, entry_idx, entry, body);
        if row_y.saturating_add(row_height) > body_bottom {
            break;
        }
        if matches!(entry, WorkspaceListEntry::SettledShowMore) {
            return Some(Rect::new(body.x, row_y, body.width, 1));
        }
        row_y = row_y.saturating_add(row_height).saturating_add(gap).min(body_bottom);
    }
    None
}

pub(crate) fn workspace_drop_indicator_row(
    app: &AppState,
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
    target: crate::app::state::WorkspaceDropTarget,
) -> Option<u16> {
    workspace_drop_slots(app, cards, area)
        .into_iter()
        .find_map(|(candidate, row)| (candidate == target).then_some(row))
}

pub(crate) fn workspace_drop_slots(
    app: &AppState,
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
) -> Vec<(crate::app::state::WorkspaceDropTarget, u16)> {
    if area.height == 0 || cards.is_empty() {
        return Vec::new();
    }
    let list_bottom = area.y + area.height.saturating_sub(1);
    let entries = workspace_list_entries(app);
    let entry_position = |ws_idx| {
        entries.iter().position(|entry| {
            matches!(
                entry,
                WorkspaceListEntry::Workspace {
                    ws_idx: entry_ws_idx,
                    ..
                } if *entry_ws_idx == ws_idx
            )
        })
    };
    let block_root_at = |entry_idx: usize| {
        entries[..=entry_idx]
            .iter()
            .rev()
            .find_map(|entry| match entry {
                WorkspaceListEntry::Workspace { ws_idx, .. } => Some(*ws_idx),
                WorkspaceListEntry::SettledHeader | WorkspaceListEntry::SettledShowMore => None,
            })
    };

    let mut slots = Vec::new();
    let mut previous_root = None;
    for card in cards {
        let Some(entry_idx) = entry_position(card.ws_idx) else {
            continue;
        };
        let Some(root_idx) = block_root_at(entry_idx) else {
            continue;
        };
        if previous_root == Some(root_idx) {
            continue;
        }
        previous_root = Some(root_idx);
        if let Some(row) = card.rect.y.checked_sub(1).filter(|row| *row < list_bottom) {
            slots.push((
                crate::app::state::WorkspaceDropTarget::Before(root_idx),
                row,
            ));
        }
    }

    let Some(last) = cards.last() else {
        return slots;
    };
    let Some(last_entry_idx) = entry_position(last.ws_idx) else {
        return slots;
    };
    let next_workspace = entries[last_entry_idx.saturating_add(1)..]
        .iter()
        .find_map(|entry| match entry {
            WorkspaceListEntry::Workspace { ws_idx, .. } => Some(*ws_idx),
            WorkspaceListEntry::SettledHeader | WorkspaceListEntry::SettledShowMore => None,
        });
    let target = match next_workspace {
        Some(ws_idx) => crate::app::state::WorkspaceDropTarget::Before(ws_idx),
        None => crate::app::state::WorkspaceDropTarget::End,
    };
    let row = last.rect.y.saturating_add(last.rect.height);
    if row < list_bottom
        && slots
            .last()
            .is_none_or(|(last_target, _)| *last_target != target)
    {
        slots.push((target, row));
    }
    slots
}
/// Relative wall-clock age for sidebar rows, in the reference app's style:
/// `now`, `23m`, `3h`, `7d`, `2w`, `4mo`.
fn relative_time(now_secs: i64, then_secs: i64) -> String {
    let diff = (now_secs - then_secs).max(0);
    match diff {
        0..=59 => "now".to_string(),
        60..=3599 => format!("{}m", diff / 60),
        3600..=86399 => format!("{}h", diff / 3600),
        86400..=604799 => format!("{}d", diff / 86400),
        604800..=2678399 => format!("{}w", diff / 604800),
        _ => format!("{}mo", diff / 2678400),
    }
}

/// Row age label: settled rows show time since settled, active rows show time
/// since last activity.
fn session_row_age(ws: &crate::workspace::Workspace) -> Option<String> {
    let then = ws.settled.or(ws.last_activity)?;
    Some(relative_time(crate::workspace::now_unix_secs(), then))
}

pub(super) fn render_sidebar(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(p.sidebar_bg));
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let sep_style = if is_navigating {
        Style::default().fg(p.accent)
    } else {
        Style::default().fg(p.surface_dim)
    };

    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let (ws_area, _detail_area) = expanded_sidebar_sections(area, app.sidebar_section_split);

    render_workspace_list(app, terminal_runtimes, frame, ws_area, is_navigating);
    render_sidebar_toggle(app, frame, area, false, p);
}

fn render_focused_card(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let height = focused_card_height(app);
    if height == 0 || area.height < 2 + height {
        return;
    }
    let p = &app.palette;
    let card_area = Rect::new(area.x, area.y + WORKSPACE_SECTION_HEADER_ROWS, area.width, height);

    let focused = app.active.or(Some(app.selected)).unwrap_or(0);
    let Some(ws) = app.workspaces.get(focused) else {
        return;
    };
    if ws.is_settled() {
        return;
    }

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(p.surface1)),
        card_area,
    );
    let inner = card_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let title = ws
        .goal
        .clone()
        .unwrap_or_else(|| ws.display_name_from(&app.terminals, terminal_runtimes));
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            truncate_end(&title, inner.width as usize),
            Style::default().fg(p.text).add_modifier(Modifier::BOLD),
        )])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let branch = ws
        .branch()
        .map(|branch| format!("⎇ {branch}"))
        .unwrap_or_else(|| "⎇ no branch".to_string());
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            truncate_end(&branch, inner.width as usize),
            Style::default().fg(p.subtext0),
        )])),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    if inner.height >= 3 {
        let mut chips = Vec::new();
        for entry in agent_panel_entries_from(app, terminal_runtimes) {
            if entry.ws_idx != focused {
                continue;
            }
            let kind = entry
                .agent_kind_label
                .clone()
                .or(entry.agent_label.clone())
                .unwrap_or_else(|| "agent".to_string());
            let (icon, style) = state_icon(entry.state, entry.seen, app.status_indicators, p);
            chips.push(Span::styled(
                format!("{icon} {kind}"),
                Style::default().fg(p.subtext0),
            ));
            chips.push(Span::styled(
                format!(" {} ", state_label(entry.state, entry.seen)),
                style,
            ));
            if chips.len() >= 6 {
                break;
            }
        }
        if !chips.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(chips)),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
            );
        }
    }
}fn render_workspace_list(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    is_navigating: bool,
) {
    let p = &app.palette;

    let list_bottom = area.y + area.height.saturating_sub(1);
    if area.height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                " sessions",
                Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
            )])),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    render_focused_card(app, terminal_runtimes, frame, area);

    let metrics = workspace_list_scroll_metrics(app, area);
    let scrollbar_rect = workspace_list_scrollbar_rect(app, area);
    let cards = &app.view.workspace_card_areas;
    let entries = workspace_list_entries(app);
    let body = workspace_list_body_rect(app, area, should_show_scrollbar(metrics));
    if body == Rect::default() {
        return;
    }

    let scroll = app.workspace_scroll.min(metrics.max_offset_from_bottom);
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut card_i = 0usize;

    for (entry_idx, entry) in entries.iter().enumerate().skip(scroll) {
        let (row_height, gap) = entry_height_and_gap(app, &entries, entry_idx, entry, body);
        if row_y.saturating_add(row_height) > body_bottom {
            break;
        }
        match entry {
            WorkspaceListEntry::Workspace { ws_idx, .. } => {
                let Some(ws) = app.workspaces.get(*ws_idx) else {
                    continue;
                };
                while card_i < cards.len() && cards[card_i].rect.y < row_y {
                    card_i += 1;
                }
                let _card = cards.get(card_i).filter(|card| card.rect.y == row_y);
                let selected = *ws_idx == app.selected && is_navigating;
                let is_active = Some(*ws_idx) == app.active;
                let is_settled = ws.is_settled();
                let dim = is_settled && !is_active && !selected;

                let bg = if selected {
                    Some(p.selection_bg)
                } else if is_active {
                    Some(p.active_row_bg)
                } else {
                    None
                };
                if let Some(bg) = bg {
                    let buf = frame.buffer_mut();
                    for y in row_y..row_y + row_height {
                        if y >= list_bottom {
                            break;
                        }
                        for x in area.x..area.x + area.width {
                            buf[(x, y)].set_style(Style::default().bg(bg));
                        }
                    }
                }

                let name_color = if dim {
                    p.overlay0
                } else if is_active || selected {
                    p.text
                } else {
                    p.subtext0
                };
                let name_style = Style::default()
                    .fg(name_color)
                    .add_modifier(Modifier::BOLD);

                let title = ws
                    .goal
                    .clone()
                    .unwrap_or_else(|| {
                        ws.display_name_from(&app.terminals, terminal_runtimes)
                    });
                let time = session_row_age(ws).unwrap_or_default();
                let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);
                let (mark, mark_style) = if is_settled {
                    ("·", Style::default().fg(p.overlay0))
                } else {
                    state_icon(agg_state, agg_seen, app.status_indicators, p)
                };

                let mut line1 = vec![Span::raw(" "), Span::styled(mark, mark_style), Span::raw(" ")];
                let time_style = Style::default().fg(if dim { p.surface_dim } else { p.overlay0 });
                let title_width = body
                    .width
                    .saturating_sub(display_width_u16(&time) + 3) as usize;
                line1.push(Span::styled(truncate_end(&title, title_width), name_style));
                while display_width(
                    &line1
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect::<String>(),
                ) < body.width.saturating_sub(display_width_u16(&time)) as usize
                {
                    line1.push(Span::raw(" "));
                }
                line1.push(Span::styled(&time, time_style));
                frame.render_widget(
                    Paragraph::new(Line::from(line1)),
                    Rect::new(area.x, row_y, body.width, 1),
                );

                let mut line2 = vec![Span::raw("  ")];
                let branch = ws.branch().unwrap_or_else(|| "main".to_string());
                let repo = ws
                    .worktree_space()
                    .map(|space| space.label.clone())
                    .or_else(|| ws.branch().map(|_| ws.display_name_from(&app.terminals, terminal_runtimes)));
                let branch_style = Style::default().fg(if dim { p.surface_dim } else { p.overlay0 });
                let repo_style = Style::default().fg(if dim { p.surface_dim } else { p.overlay0 });
                line2.push(Span::styled(
                    format!("⎇ {branch}"),
                    branch_style,
                ));
                if let Some(repo) = repo.filter(|repo| repo != &branch) {
                    let repo_width = display_width_u16(&repo);
                    let branch_width = display_width_u16(&format!("⎇ {branch}"));
                    let padding = body
                        .width
                        .saturating_sub(2 + branch_width + repo_width);
                    line2.push(Span::raw(" ".repeat(padding as usize)));
                    line2.push(Span::styled(repo, repo_style));
                }
                frame.render_widget(
                    Paragraph::new(Line::from(line2)),
                    Rect::new(area.x, row_y + 1, body.width, 1),
                );
                card_i += 1;
            }
            WorkspaceListEntry::SettledHeader => {
                frame.render_widget(
                    Paragraph::new(Line::from(vec![Span::styled(
                        " Settled",
                        Style::default()
                            .fg(p.overlay0)
                            .add_modifier(Modifier::BOLD),
                    )])),
                    Rect::new(area.x, row_y, body.width, 1),
                );
            }
            WorkspaceListEntry::SettledShowMore => {
                let settled_count = app
                    .workspaces
                    .iter()
                    .filter(|ws| ws.is_settled())
                    .count();
                let hidden = settled_count.saturating_sub(app.sidebar_spaces.settled_preview);
                frame.render_widget(
                    Paragraph::new(Line::from(vec![Span::styled(
                        format!(" + Show {hidden} more"),
                        Style::default().fg(p.blue),
                    )])),
                    Rect::new(area.x, row_y, body.width, 1),
                );
            }
        }
        row_y = row_y.saturating_add(row_height).saturating_add(gap).min(body_bottom);
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }

    if app.mouse_capture && list_bottom > area.y {
        let new_rect = app.sidebar_new_button_rect();
        frame.render_widget(
            Paragraph::new(Span::styled(" new", Style::default().fg(p.overlay0))),
            new_rect,
        );

        let menu_rect = app.global_launcher_rect();
        let menu_line = if app.global_menu_attention_badge_visible() {
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled("menu", Style::default().fg(p.overlay0)),
            ])
        } else {
            Line::from(vec![Span::styled("menu", Style::default().fg(p.overlay0))])
        };
        frame.render_widget(
            Paragraph::new(menu_line).alignment(Alignment::Right),
            menu_rect,
        );
    }
}
pub(crate) fn collapsed_sidebar_toggle_rect(area: Rect) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    let x = area.x + content_w / 2;
    Rect::new(x, bottom_y, 1, 1)
}

pub(crate) fn expanded_sidebar_toggle_rect(area: Rect) -> Rect {
    if area.width <= 1 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(
        area.x + area.width.saturating_sub(2),
        area.y + area.height.saturating_sub(1),
        1,
        1,
    )
}

fn render_sidebar_toggle(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    collapsed: bool,
    p: &Palette,
) {
    let toggle_area = if collapsed {
        collapsed_sidebar_toggle_rect(area)
    } else {
        expanded_sidebar_toggle_rect(area)
    };
    if toggle_area == Rect::default() {
        return;
    }
    let icon = if collapsed { "»" } else { "«" };
    let icon_style = if collapsed && app.global_menu_attention_badge_visible() {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay0)
    };
    frame.render_widget(Paragraph::new(Span::styled(icon, icon_style)), toggle_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::state::AppState,
        detect::Agent,
        workspace::{now_unix_secs, Workspace},
    };
    use ratatui::{backend::TestBackend, Terminal};

    fn session_app(specs: &[(&str, Option<i64>, Option<i64>)]) -> AppState {
        // (name, last_activity, settled)
        let mut state = AppState::test_new();
        state.workspaces = specs
            .iter()
            .map(|(name, activity, settled)| {
                let mut ws = Workspace::test_new(name);
                ws.last_activity = *activity;
                ws.settled = *settled;
                ws
            })
            .collect();
        state.active = Some(0);
        state.selected = 0;
        state
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn relative_time_buckets_cover_reference_styles() {
        let now = 1_000_000;
        assert_eq!(relative_time(now, now), "now");
        assert_eq!(relative_time(now, now - 59), "now");
        assert_eq!(relative_time(now, now - 60), "1m");
        assert_eq!(relative_time(now, now - 3_540), "59m");
        assert_eq!(relative_time(now, now - 3_600), "1h");
        assert_eq!(relative_time(now, now - 82_800), "23h");
        assert_eq!(relative_time(now, now - 86_400), "1d");
        assert_eq!(relative_time(now, now - 604_800), "1w");
        assert_eq!(relative_time(now, now - 2_678_400), "1mo");
    }

    #[test]
    fn session_entries_sort_active_by_recency_desc() {
        let state = session_app(&[
            ("older", Some(100), None),
            ("newest", Some(300), None),
            ("middle", Some(200), None),
        ]);
        let entries = workspace_list_entries(&state);
        let ws_indices: Vec<usize> = entries
            .iter()
            .filter_map(|entry| match entry {
                WorkspaceListEntry::Workspace { ws_idx, .. } => Some(*ws_idx),
                _ => None,
            })
            .collect();
        assert_eq!(ws_indices, vec![1, 2, 0]);
    }

    #[test]
    fn settled_sessions_trail_header_and_collapse_to_show_more() {
        let state = session_app(&[
            ("active", Some(100), None),
            ("s1", Some(10), Some(50)),
            ("s2", Some(10), Some(40)),
            ("s3", Some(10), Some(30)),
            ("s4", Some(10), Some(20)),
            ("s5", Some(10), Some(10)),
        ]);
        // Default settled_preview collapses to 3 preview rows.
        assert_eq!(state.sidebar_spaces.settled_preview, 3);
        let entries = workspace_list_entries(&state);
        assert_eq!(entries[0], WorkspaceListEntry::Workspace { ws_idx: 0, indented: false });
        assert_eq!(entries[1], WorkspaceListEntry::SettledHeader);
        assert_eq!(entries[2], WorkspaceListEntry::Workspace { ws_idx: 1, indented: false });
        assert_eq!(entries[3], WorkspaceListEntry::Workspace { ws_idx: 2, indented: false });
        assert_eq!(entries[4], WorkspaceListEntry::Workspace { ws_idx: 3, indented: false });
        assert_eq!(entries[5], WorkspaceListEntry::SettledShowMore);
        assert_eq!(entries.len(), 6);

        let mut expanded = state;
        expanded.settled_expanded = true;
        let entries = workspace_list_entries(&expanded);
        assert!(!entries.contains(&WorkspaceListEntry::SettledShowMore));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry, WorkspaceListEntry::Workspace { .. }))
                .count(),
            6
        );
        assert_eq!(entries.len(), 7);
    }

    #[test]
    fn settled_preview_is_read_from_config() {
        let mut state = session_app(&[
            ("active", Some(100), None),
            ("s1", Some(10), Some(50)),
            ("s2", Some(10), Some(40)),
            ("s3", Some(10), Some(30)),
            ("s4", Some(10), Some(20)),
            ("s5", Some(10), Some(10)),
        ]);
        state.sidebar_spaces.settled_preview = 2;

        let entries = workspace_list_entries(&state);
        assert_eq!(entries[0], WorkspaceListEntry::Workspace { ws_idx: 0, indented: false });
        assert_eq!(entries[1], WorkspaceListEntry::SettledHeader);
        assert_eq!(entries[2], WorkspaceListEntry::Workspace { ws_idx: 1, indented: false });
        assert_eq!(entries[3], WorkspaceListEntry::Workspace { ws_idx: 2, indented: false });
        assert_eq!(entries[4], WorkspaceListEntry::SettledShowMore);
        assert_eq!(entries.len(), 5);

        // A preview at least as large as the settled count never collapses.
        state.sidebar_spaces.settled_preview = 10;
        let entries = workspace_list_entries(&state);
        assert!(!entries.contains(&WorkspaceListEntry::SettledShowMore));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry, WorkspaceListEntry::Workspace { .. }))
                .count(),
            6
        );
    }

    #[test]
    fn expanded_sidebar_sections_return_full_content_without_detail_area() {
        let (ws_area, detail_area) = expanded_sidebar_sections(Rect::new(0, 0, 20, 5), 0.9);
        assert_eq!(ws_area, Rect::new(0, 0, 19, 5));
        assert_eq!(detail_area, Rect::default());
    }

    #[test]
    fn session_rows_render_mark_goal_time_and_branch_repo() {
        let mut state = session_app(&[("repo-work", Some(now_unix_secs() - 120), None)]);
        state.workspaces[0].goal = Some("fix billing bug".into());
        state.workspaces[0].cached_git_branch = Some("main".into());
        state.active = None;
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.sidebar_collapsed = false;

        let area = Rect::new(0, 0, 30, 12);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar(&state, &TerminalRuntimeRegistry::default(), frame, area))
            .expect("sidebar should render");
        let buffer = terminal.backend().buffer();

        let line1 = row_text(buffer, 6, area.width);
        assert!(line1.contains("fix billing bug"), "line1: {line1}");
        assert!(line1.contains("2m"), "line1: {line1}");
        let line2 = row_text(buffer, 7, area.width);
        assert!(line2.contains("⎇ main"), "line2: {line2}");
        assert!(line2.contains("repo-work"), "line2: {line2}");
    }

    #[test]
    fn settled_rows_render_dim_mark_and_header() {
        let mut state = session_app(&[("done-thing", Some(10), Some(now_unix_secs() - 5))]);
        state.workspaces[0].goal = Some("finished task".into());
        state.active = None;
        state.mode = Mode::Terminal;

        let area = Rect::new(0, 0, 30, 14);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar(&state, &TerminalRuntimeRegistry::default(), frame, area))
            .expect("sidebar should render");
        let buffer = terminal.backend().buffer();

        // header row (" sessions"), then no focused card (active=None), then
        // the Settled header, then the settled row.
        assert!(row_text(buffer, 2, area.width).contains("Settled"));
        let row = row_text(buffer, 3, area.width);
        assert!(row.contains("·"), "settled row should use dim dot: {row}");
        assert!(row.contains("finished task"), "row: {row}");
    }

    #[test]
    fn focused_card_renders_goal_branch_and_agent_chips() {
        let mut state = session_app(&[("cardy", Some(now_unix_secs()), None)]);
        state.workspaces[0].goal = Some("refund idempotency".into());
        state.workspaces[0].cached_git_branch = Some("worktree/fix-refunds".into());
        state.active = Some(0);
        state.mode = Mode::Terminal;
        state.ensure_test_terminals();
        if let Some(terminal) = state.terminals.values_mut().next() {
            terminal.agent_name = Some("claude".to_string());
            terminal.detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 36, 16);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar(&state, &TerminalRuntimeRegistry::default(), frame, area))
            .expect("sidebar should render");
        let buffer = terminal.backend().buffer();

        let title = row_text(buffer, 3, area.width);
        assert!(title.contains("refund idempotency"), "title: {title}");
        let branch = row_text(buffer, 4, area.width);
        assert!(branch.contains("⎇ worktree/fix-refunds"), "branch: {branch}");
        let chips = row_text(buffer, 5, area.width);
        assert!(chips.contains("claude"), "chips: {chips}");
    }

    #[test]
    fn collapsed_sidebar_renders_display_order_with_dim_settled_rows() {
        let mut state = session_app(&[
            ("oldest", Some(10), None),
            ("newest", Some(300), None),
            ("settled-one", Some(5), Some(100)),
        ]);
        state.active = None;
        state.mode = Mode::Terminal;
        state.sidebar_collapsed = true;

        let area = Rect::new(0, 0, 8, 10);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar_collapsed(&state, frame, area))
            .expect("collapsed sidebar should render");
        let buffer = terminal.backend().buffer();

        // Display order: newest, oldest, settled. Numbers follow that order.
        let row0 = row_text(buffer, 0, 6);
        assert!(row0.contains('1'), "row0: {row0}");
        let row2 = row_text(buffer, 2, 6);
        assert!(row2.contains('3') && row2.contains('·'), "row2: {row2}");
    }

    #[test]
    fn expanded_and_collapsed_sidebars_use_custom_background() {
        let mut state = session_app(&[("one", None, None)]);
        state.active = None;
        let area = Rect::new(0, 0, 30, 12);

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar(&state, &TerminalRuntimeRegistry::default(), frame, area))
            .expect("expanded sidebar should render");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(area.width - 1, 0)].symbol(), "│");
        assert_eq!(
            buffer[(0, 0)].bg,
            buffer[(area.width - 2, area.height - 2)].bg
        );

        state.sidebar_collapsed = true;
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar_collapsed(&state, frame, area))
            .expect("collapsed sidebar should render");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(area.width - 1, 0)].symbol(), "│");
    }

    #[test]
    fn render_sidebar_toggle_draws_expanded_collapse_icon() {
        let mut state = session_app(&[("one", None, None)]);
        state.active = None;
        let area = Rect::new(0, 0, 30, 12);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar(&state, &TerminalRuntimeRegistry::default(), frame, area))
            .expect("sidebar toggle should render");

        let toggle = expanded_sidebar_toggle_rect(area);
        assert_eq!(
            terminal.backend().buffer()[(toggle.x, toggle.y)].symbol(),
            "«"
        );
    }

    #[test]
    fn expanded_sidebar_toggle_sits_inside_sidebar_content() {
        let area = Rect::new(0, 0, 26, 20);
        let toggle = expanded_sidebar_toggle_rect(area);

        assert_eq!(toggle.x, area.x + area.width - 2);
        assert_eq!(toggle.y, area.y + area.height - 1);
    }
}
