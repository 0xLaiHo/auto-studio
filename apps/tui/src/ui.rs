use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use tui_big_text::{BigText, PixelSize};

use crate::app::{App, Overlay, TextInputKind};
use crate::constants::{
    THEME_CANVAS, THEME_DANGER, THEME_HIGHLIGHT, THEME_INK, THEME_MUTED, THEME_OVERLAY,
    THEME_PRIMARY, THEME_SECONDARY, THEME_SUCCESS, THEME_SURFACE, THEME_TEXT, THEME_WARNING,
};
use crate::model::{
    LlmModelCatalogStateView, ThinkingCapabilityView, ThinkingControlView, ThinkingLevelView,
};

const HOME_WIDTH: u16 = 74;
const HOME_HEIGHT: u16 = 18;
const HOME_VERTICAL_OFFSET: u16 = 3;
const COMPOSER_HEIGHT: u16 = 5;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(THEME_CANVAS)),
        area,
    );
    let root = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    draw_home(frame, root[0], app);
    draw_footer(frame, root[1], app);
    draw_overlay(frame, area, app);
}

fn draw_home(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let width = area.width.saturating_sub(4).min(HOME_WIDTH);
    let height = area.height.min(HOME_HEIGHT);
    let mut content = centered_rect(width, height, area);
    content.y = content
        .y
        .saturating_add(HOME_VERTICAL_OFFSET.min(area.bottom().saturating_sub(content.bottom())));
    let rows = Layout::vertical([
        Constraint::Length(6),
        Constraint::Length(COMPOSER_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(content);

    let brand = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .alignment(Alignment::Center)
        .lines(vec![Line::from(vec![
            Span::styled("AUTO", Style::default().fg(THEME_SECONDARY)),
            Span::styled("STUDIO", Style::default().fg(THEME_PRIMARY)),
        ])])
        .build();
    frame.render_widget(brand, rows[0]);

    draw_composer(frame, rows[1], app);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("ctrl+p", Style::default().fg(THEME_PRIMARY)),
            Span::styled(" commands   ", Style::default().fg(THEME_MUTED)),
            Span::styled("/connect", Style::default().fg(THEME_PRIMARY)),
            Span::styled(" provider   ", Style::default().fg(THEME_MUTED)),
            Span::styled("/model", Style::default().fg(THEME_PRIMARY)),
            Span::styled(" models", Style::default().fg(THEME_MUTED)),
        ]))
        .style(Style::default().bg(THEME_CANVAS)),
        rows[3],
    );
    draw_activity(frame, rows[5], app);
}

fn draw_activity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if let Some(project) = &app.project {
        if let Some(brief) = &project.brief {
            lines.push(Line::from(vec![
                Span::styled("Brief  ", Style::default().fg(THEME_SECONDARY)),
                Span::raw(&brief.summary),
            ]));
        }
        if let Some(run) = project.agent_runs.last() {
            lines.push(Line::from(vec![
                Span::styled("Run    ", Style::default().fg(THEME_SECONDARY)),
                Span::raw(format!("{:?}", run.status)),
                Span::styled("  ·  ", Style::default().fg(THEME_MUTED)),
                Span::raw(&run.plan.visible_summary),
            ]));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("• Tip  ", Style::default().fg(THEME_WARNING)),
            Span::styled(
                "Connect a Provider, select a model, then start creating.",
                Style::default().fg(THEME_MUTED),
            ),
        ]));
    }
    if !app.logs.is_empty() {
        lines.push(Line::raw(""));
        lines.extend(app.logs.iter().rev().take(3).map(|message| {
            Line::from(vec![
                Span::styled("• ", Style::default().fg(THEME_HIGHLIGHT)),
                Span::raw(message),
            ])
        }));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(Block::default().padding(Padding::left(7)))
            .style(Style::default().fg(THEME_TEXT).bg(THEME_CANVAS)),
        area,
    );
}

fn draw_composer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let input = if app.composer.is_empty() {
        Line::styled(
            "Ask Auto Studio anything, or type / for commands",
            Style::default().fg(THEME_MUTED),
        )
    } else {
        Line::from(vec![
            Span::raw(&app.composer),
            Span::styled("_", Style::default().fg(THEME_PRIMARY)),
        ])
    };
    let status = connection_label(app);
    let text = Text::from(vec![input, Line::raw(""), status]);
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(THEME_PRIMARY))
                    .padding(Padding::new(2, 2, 1, 0)),
            )
            .style(Style::default().fg(THEME_TEXT).bg(THEME_SURFACE)),
        area,
    );
}

fn connection_label(app: &App) -> Line<'static> {
    let Some(status) = &app.provider_status else {
        return Line::styled(
            "Core connected · checking Provider",
            Style::default().fg(THEME_MUTED),
        );
    };
    if !status.configured {
        return Line::from(vec![
            Span::styled("No Provider", Style::default().fg(THEME_WARNING).bold()),
            Span::styled("  ·  /connect", Style::default().fg(THEME_MUTED)),
        ]);
    }
    let provider = status
        .provider_kind
        .as_deref()
        .unwrap_or("Provider")
        .to_owned();
    match status.catalog.state {
        LlmModelCatalogStateView::Refreshing => Line::from(vec![
            Span::styled(provider, Style::default().fg(THEME_PRIMARY)),
            Span::styled("  ·  fetching models…", Style::default().fg(THEME_MUTED)),
        ]),
        LlmModelCatalogStateView::Failed => Line::from(vec![
            Span::styled(provider, Style::default().fg(THEME_PRIMARY)),
            Span::styled(
                "  ·  model refresh failed",
                Style::default().fg(THEME_DANGER),
            ),
            Span::styled("  ·  /refresh-models", Style::default().fg(THEME_MUTED)),
        ]),
        _ => {
            let selected_model = status.model.clone();
            let has_model = selected_model.is_some();
            let mut spans = vec![
                Span::styled(provider, Style::default().fg(THEME_PRIMARY)),
                Span::styled("  ·  ", Style::default().fg(THEME_MUTED)),
                Span::styled(
                    selected_model.unwrap_or_else(|| "select model with /model".to_owned()),
                    if has_model {
                        Style::default().fg(THEME_TEXT)
                    } else {
                        Style::default().fg(THEME_WARNING)
                    },
                ),
            ];
            if has_model {
                spans.push(Span::styled("  ·  ", Style::default().fg(THEME_MUTED)));
                spans.push(Span::styled(
                    status.thinking_level.compact_label(),
                    Style::default().fg(THEME_HIGHLIGHT).bold(),
                ));
            }
            Line::from(spans)
        }
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let project = app.project.as_ref().map_or_else(
        || "~".to_owned(),
        |project| format!("{} · rev {}", project.name, project.revision),
    );
    let version = env!("CARGO_PKG_VERSION");
    let columns = Layout::horizontal([Constraint::Min(1), Constraint::Length(12)]).split(area);
    frame.render_widget(
        Paragraph::new(format!("  {project}"))
            .style(Style::default().fg(THEME_MUTED).bg(THEME_CANVAS)),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(version)
            .alignment(Alignment::Right)
            .style(Style::default().fg(THEME_MUTED).bg(THEME_CANVAS)),
        columns[1],
    );
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, app: &App) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::Commands { selected } => draw_commands(frame, area, app, *selected),
        Overlay::Providers { query, selected } => {
            draw_providers(frame, area, app, query, *selected);
        }
        Overlay::ApiKey { provider, value } => draw_api_key(frame, area, provider, value),
        Overlay::Models {
            query,
            selected,
            thinking_level,
        } => draw_models(frame, area, app, query, *selected, *thinking_level),
        Overlay::TextInput { kind, value } => draw_text_input(frame, area, *kind, value),
        Overlay::Help => draw_help(frame, area),
    }
}

fn draw_commands(frame: &mut Frame<'_>, area: Rect, app: &App, selected: usize) {
    let commands = app.filtered_commands();
    let popup = popup_rect(
        74,
        saturating_u16(commands.len()).saturating_add(7).min(24),
        area,
    );
    let items = commands.iter().enumerate().map(|(index, command)| {
        selected_item(index == selected, command.name, command.description)
    });
    draw_list_popup(
        frame,
        popup,
        "Commands",
        app.composer.as_str(),
        selected,
        items,
    );
}

fn draw_providers(frame: &mut Frame<'_>, area: Rect, app: &App, query: &str, selected: usize) {
    let providers = app.filtered_providers(query);
    let popup = popup_rect(
        62,
        saturating_u16(providers.len()).saturating_add(6).max(10),
        area,
    );
    let items = providers.iter().enumerate().map(|(index, provider)| {
        let connected = app
            .provider_status
            .as_ref()
            .is_some_and(|status| status.provider_kind.as_deref() == Some(provider.id.as_str()));
        let detail = if connected {
            "connected"
        } else {
            provider.id.as_str()
        };
        selected_item(index == selected, &provider.display_name, detail)
    });
    draw_list_popup(frame, popup, "Connect a provider", query, selected, items);
}

fn draw_models(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    query: &str,
    selected: usize,
    thinking_level: ThinkingLevelView,
) {
    let models = app.filtered_models(query);
    frame.render_widget(Clear, area);
    let panel = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(THEME_SECONDARY))
        .style(Style::default().fg(THEME_TEXT).bg(THEME_SURFACE));
    let inner = panel.inner(area).inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 0,
    });
    frame.render_widget(panel, area);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(model_selector_title(query)), rows[0]);
    frame.render_widget(
        Paragraph::new("↑↓ navigate · ←→ thinking · Enter select · Esc cancel")
            .style(Style::default().fg(THEME_MUTED)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(
            "Note: Changing models may invalidate Provider prompt caches and increase token costs.",
        )
        .style(Style::default().fg(THEME_WARNING)),
        rows[2],
    );

    let provider = app
        .provider_status
        .as_ref()
        .and_then(|status| status.provider_kind.as_deref())
        .unwrap_or("Provider");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {provider} "),
            Style::default().fg(THEME_INK).bg(THEME_PRIMARY).bold(),
        ))),
        rows[4],
    );

    if models.is_empty() {
        frame.render_widget(
            Paragraph::new(model_catalog_message(app))
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(THEME_DANGER)),
            rows[6],
        );
    } else {
        draw_model_rows(frame, rows[6], app, &models, selected, provider);
    }

    let capability = models
        .get(selected)
        .map_or_else(ThinkingCapabilityView::default, |model| {
            model.thinking.clone()
        });
    frame.render_widget(
        Paragraph::new(thinking_title(capability.control)).style(Style::default().fg(THEME_MUTED)),
        rows[8],
    );
    frame.render_widget(
        Paragraph::new(thinking_level_line(&capability.levels, thinking_level)),
        rows[9],
    );
}

fn model_selector_title(query: &str) -> Line<'_> {
    if query.is_empty() {
        Line::from(vec![
            Span::styled("Select a model", Style::default().fg(THEME_PRIMARY).bold()),
            Span::styled("  (type to search)", Style::default().fg(THEME_MUTED)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Select a model", Style::default().fg(THEME_PRIMARY).bold()),
            Span::styled(format!("  {query}"), Style::default().fg(THEME_TEXT)),
            Span::styled("_", Style::default().fg(THEME_HIGHLIGHT)),
        ])
    }
}

fn model_catalog_message(app: &App) -> String {
    app.provider_status.as_ref().map_or(
        "Connect a Provider first with /connect".to_owned(),
        |status| match status.catalog.state {
            LlmModelCatalogStateView::Refreshing => "Fetching models…".to_owned(),
            LlmModelCatalogStateView::Failed => status
                .catalog
                .error
                .clone()
                .unwrap_or_else(|| "Model catalog refresh failed".to_owned()),
            _ => "No models match this search".to_owned(),
        },
    )
}

fn draw_model_rows(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    models: &[&crate::model::LlmModelView],
    selected: usize,
    provider: &str,
) {
    let items = models.iter().enumerate().map(|(index, model)| {
        let current = app
            .provider_status
            .as_ref()
            .and_then(|status| status.model.as_deref())
            == Some(model.id.as_str());
        let display_name = model.display_name.chars().take(28).collect::<String>();
        let mut spans = vec![
            Span::raw(format!("{display_name:<30}")),
            Span::styled(format!("{provider:<18}"), Style::default().fg(THEME_MUTED)),
        ];
        if current {
            spans.push(Span::styled(
                "← current",
                Style::default().fg(THEME_SUCCESS),
            ));
        }
        ListItem::new(Line::from(spans)).style(if index == selected {
            Style::default().fg(THEME_HIGHLIGHT).bold()
        } else {
            Style::default().fg(THEME_TEXT)
        })
    });
    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(THEME_HIGHLIGHT).bold());
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn thinking_title(control: ThinkingControlView) -> &'static str {
    match control {
        ThinkingControlView::Unsupported => "Thinking  Provider controlled · ←→ unavailable",
        ThinkingControlView::Toggle => "Thinking  On / Off · ←→ to switch",
        ThinkingControlView::Effort => "Thinking effort  ←→ to switch",
        ThinkingControlView::AdaptiveEffort => "Adaptive thinking effort  ←→ to switch",
        ThinkingControlView::TokenBudget => "Thinking budget  ←→ to switch",
    }
}

fn thinking_level_line(levels: &[ThinkingLevelView], selected: ThinkingLevelView) -> Line<'static> {
    let mut spans = Vec::new();
    for level in levels {
        let style = if *level == selected {
            Style::default().fg(THEME_HIGHLIGHT).bold()
        } else {
            Style::default().fg(THEME_TEXT)
        };
        let label = if *level == selected {
            format!("[ {} ]", level.label())
        } else {
            format!("  {}  ", level.label())
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw("   "));
    }
    Line::from(spans)
}

fn draw_api_key(
    frame: &mut Frame<'_>,
    area: Rect,
    provider: &crate::model::LlmProviderView,
    value: &str,
) {
    let popup = popup_rect(58, 12, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default().style(Style::default().bg(THEME_OVERLAY)),
        popup,
    );
    let inner = popup.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let lines = vec![
        Line::styled(
            format!("{} API key", provider.display_name),
            Style::default().fg(THEME_PRIMARY).bold(),
        ),
        Line::raw(""),
        Line::styled(
            format!("{} _", "•".repeat(value.chars().count())),
            Style::default().fg(THEME_TEXT),
        ),
        Line::raw(""),
        Line::styled(
            "Saved only in Auto Studio's private local configuration.",
            Style::default().fg(THEME_MUTED),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::raw("enter"),
            Span::styled(" save   esc cancel", Style::default().fg(THEME_MUTED)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_text_input(frame: &mut Frame<'_>, area: Rect, kind: TextInputKind, value: &str) {
    let title = match kind {
        TextInputKind::ProjectName => "Project name",
        TextInputKind::Brief => "Creative Brief",
        TextInputKind::ApprovalBudget => "Approval budget · USD minor units",
    };
    let popup = popup_rect(62, 10, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default().style(Style::default().bg(THEME_OVERLAY)),
        popup,
    );
    let inner = popup.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(title, Style::default().fg(THEME_PRIMARY).bold()),
            Line::raw(""),
            Line::from(format!("{value}_")),
            Line::raw(""),
            Line::styled(
                "enter submit   esc cancel",
                Style::default().fg(THEME_MUTED),
            ),
        ]),
        inner,
    );
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = popup_rect(64, 16, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default().style(Style::default().bg(THEME_OVERLAY)),
        popup,
    );
    let inner = popup.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let help = vec![
        Line::styled("Auto Studio", Style::default().fg(THEME_PRIMARY).bold()),
        Line::raw(""),
        Line::raw("/                Open and filter commands"),
        Line::raw("/connect         Save a Provider API key"),
        Line::raw("/model           Select a fetched model"),
        Line::raw("/refresh-models  Fetch the catalog again"),
        Line::raw("/exit            Exit Auto Studio"),
        Line::raw(""),
        Line::raw("up/down          Move selection"),
        Line::raw("enter            Confirm"),
        Line::raw("esc              Close overlay"),
        Line::raw("ctrl+p           Open commands"),
        Line::raw("ctrl+c           Exit"),
    ];
    frame.render_widget(Paragraph::new(help), inner);
}

fn draw_list_popup<'a>(
    frame: &mut Frame<'_>,
    popup: Rect,
    title: &str,
    query: &str,
    selected: usize,
    items: impl IntoIterator<Item = ListItem<'a>>,
) {
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default().style(Style::default().bg(THEME_OVERLAY)),
        popup,
    );
    let inner = popup.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(THEME_PRIMARY).bold()),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(format!("Search  {query}_")).style(Style::default().fg(THEME_MUTED)),
        rows[1],
    );
    let items = items.into_iter().collect::<Vec<_>>();
    let selected = (!items.is_empty()).then_some(selected.min(items.len().saturating_sub(1)));
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(List::new(items), rows[2], &mut state);
    frame.render_widget(
        Paragraph::new("enter select   esc close").style(Style::default().fg(THEME_MUTED)),
        rows[3],
    );
}

fn selected_item<'a>(selected: bool, label: &'a str, detail: &'a str) -> ListItem<'a> {
    let style = if selected {
        Style::default().fg(THEME_INK).bg(THEME_HIGHLIGHT).bold()
    } else {
        Style::default().fg(THEME_TEXT)
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!(" {label:<24}"), style),
        Span::styled(format!(" {detail}"), style.add_modifier(Modifier::DIM)),
    ]))
    .style(style)
}

fn centered_width(width: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y,
        width,
        area.height,
    )
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let height = height.min(area.height);
    centered_width(
        width,
        Rect::new(
            area.x,
            area.y
                .saturating_add(area.height.saturating_sub(height) / 2),
            area.width,
            height,
        ),
    )
}

fn popup_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    centered_width(width, vertical[1])
}

fn saturating_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::draw;
    use crate::app::{App, Overlay};
    use crate::constants::{THEME_CANVAS, THEME_HIGHLIGHT, THEME_INK, THEME_MUTED, THEME_SURFACE};
    use crate::model::{
        LlmConnectionStatusView, LlmModelCatalogStateView, LlmModelCatalogView, LlmModelView,
        ThinkingCapabilityView, ThinkingControlView, ThinkingLevelView,
    };
    use std::collections::BTreeMap;

    #[test]
    fn home_screen_uses_the_composer_instead_of_the_old_workflow_columns() {
        let backend = TestBackend::new(116, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let app = App {
            overlay: Overlay::Commands { selected: 0 },
            composer: "/".to_owned(),
            ..App::default()
        };
        terminal.draw(|frame| draw(frame, &app)).expect("draw");
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("/connect"));
        assert!(rendered.contains("Connect an LLM provider"));
        assert!(!rendered.contains("WORKFLOW"));
        assert!(!rendered.contains("INSPECTOR"));
    }

    #[test]
    fn command_menu_keeps_exit_visible_and_scrollable() {
        let backend = TestBackend::new(90, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let app = App {
            overlay: Overlay::Commands {
                selected: crate::app::COMMANDS.len() - 1,
            },
            composer: "/".to_owned(),
            ..App::default()
        };

        terminal.draw(|frame| draw(frame, &app)).expect("draw");
        let rendered = terminal.backend().to_string();

        assert!(rendered.contains("/exit"), "{rendered}");
        assert!(rendered.contains("Exit Auto Studio"), "{rendered}");
    }

    #[test]
    fn home_screen_keeps_the_opencode_style_group_compact_and_centered() {
        let backend = TestBackend::new(116, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| draw(frame, &App::default()))
            .expect("draw");

        let rendered = terminal.backend().to_string();
        let lines = rendered.lines().collect::<Vec<_>>();
        let input_row = lines
            .iter()
            .position(|line| line.contains("Ask Auto Studio"))
            .expect("input row");
        let status_row = lines
            .iter()
            .position(|line| line.contains("Core connected"))
            .expect("status row");
        let shortcuts_row = lines
            .iter()
            .position(|line| line.contains("ctrl+p"))
            .expect("shortcuts row");

        assert_eq!(input_row, 20);
        assert_eq!(status_row.saturating_sub(input_row), 2);
        assert_eq!(shortcuts_row.saturating_sub(status_row), 3);
        let input_column = lines[input_row]
            .split_once("Ask Auto Studio")
            .expect("input column")
            .0
            .chars()
            .count();
        assert_eq!(input_column, 25, "{}", lines[input_row]);
    }

    #[test]
    fn cyberpunk_theme_colors_the_canvas_composer_and_selected_command() {
        let backend = TestBackend::new(116, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| draw(frame, &App::default()))
            .expect("draw home");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, 0)).expect("canvas cell").bg, THEME_CANVAS);
        let rendered = terminal.backend().to_string();
        let input_row = rendered
            .lines()
            .position(|line| line.contains("Ask Auto Studio"))
            .expect("input row");
        let input_column = rendered.lines().nth(input_row).expect("input line")[..]
            .find("Ask Auto Studio")
            .expect("input column");
        let input_cell = buffer
            .cell((
                u16::try_from(input_column).expect("input column fits"),
                u16::try_from(input_row).expect("input row fits"),
            ))
            .expect("composer input cell");
        assert_eq!(input_cell.fg, THEME_MUTED);
        assert_eq!(input_cell.bg, THEME_SURFACE);

        let command_app = App {
            overlay: Overlay::Commands { selected: 0 },
            composer: "/".to_owned(),
            ..App::default()
        };
        terminal
            .draw(|frame| draw(frame, &command_app))
            .expect("draw commands");
        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.bg == THEME_HIGHLIGHT && cell.fg == THEME_INK),
            "selected command must use the neon highlight token"
        );
    }

    #[test]
    fn home_status_shows_the_active_thinking_level_after_the_model() {
        let backend = TestBackend::new(116, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let app = App {
            provider_status: Some(LlmConnectionStatusView {
                configured: true,
                provider_kind: Some("deepseek".to_owned()),
                model: Some("deepseek-v4-pro".to_owned()),
                thinking_level: ThinkingLevelView::Max,
                model_thinking_levels: BTreeMap::new(),
                source: None,
                catalog: LlmModelCatalogView {
                    state: LlmModelCatalogStateView::Ready,
                    models: Vec::new(),
                    error: None,
                },
            }),
            ..App::default()
        };

        terminal.draw(|frame| draw(frame, &app)).expect("draw");
        let rendered = terminal.backend().to_string();
        let status = rendered
            .lines()
            .find(|line| line.contains("deepseek-v4-pro"))
            .expect("model status row");

        assert!(status.contains("max"), "{status}");
        assert!(
            status.find("deepseek-v4-pro") < status.find("max"),
            "{status}"
        );
    }

    #[test]
    fn model_selector_exposes_model_navigation_and_thinking_as_one_full_screen_flow() {
        let backend = TestBackend::new(116, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let app = App {
            provider_status: Some(LlmConnectionStatusView {
                configured: true,
                provider_kind: Some("deepseek".to_owned()),
                model: Some("deepseek-v4-flash".to_owned()),
                thinking_level: ThinkingLevelView::Max,
                model_thinking_levels: BTreeMap::new(),
                source: None,
                catalog: LlmModelCatalogView {
                    state: LlmModelCatalogStateView::Ready,
                    models: vec![
                        LlmModelView {
                            id: "deepseek-v4-flash".to_owned(),
                            display_name: "DeepSeek V4 Flash".to_owned(),
                            thinking: ThinkingCapabilityView {
                                control: ThinkingControlView::Effort,
                                levels: vec![
                                    ThinkingLevelView::Off,
                                    ThinkingLevelView::Low,
                                    ThinkingLevelView::High,
                                    ThinkingLevelView::Max,
                                ],
                                default_level: ThinkingLevelView::High,
                            },
                        },
                        LlmModelView {
                            id: "deepseek-reasoner".to_owned(),
                            display_name: "DeepSeek Reasoner".to_owned(),
                            thinking: ThinkingCapabilityView::default(),
                        },
                    ],
                    error: None,
                },
            }),
            overlay: Overlay::Models {
                query: String::new(),
                selected: 0,
                thinking_level: ThinkingLevelView::Max,
            },
            ..App::default()
        };

        terminal.draw(|frame| draw(frame, &app)).expect("draw");
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("Select a model  (type to search)"));
        assert!(rendered.contains("↑↓ navigate · ←→ thinking"));
        assert!(rendered.contains("DeepSeek V4 Flash"));
        assert!(rendered.contains("DeepSeek Reasoner"));
        assert!(rendered.contains("Thinking effort  ←→ to switch"));
        assert!(rendered.contains("[ Max ]"));
        assert!(!rendered.contains("AUTOSTUDIO"));
    }
}
