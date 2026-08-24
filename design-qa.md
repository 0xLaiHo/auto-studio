# Auto Studio TUI design QA

## Scope

- Reference: OpenCode home layout supplied by the user.
- Build under review: Auto Studio TUI home screen.
- Compared state: idle composer; connection text differs intentionally because the verification profile has no configured Provider.
- Terminal geometry is evaluated in character cells so the layout remains stable across font sizes and pixel densities.

## Geometry checks

- The home group is constrained to 74 columns instead of expanding with the terminal.
- The brand, composer, shortcuts, and tip form one fixed 18-row group with a small downward visual bias.
- The composer is five rows high with one row of top padding; the Provider/model row no longer leaves two empty rows below it.
- One blank row separates the composer from the shortcut legend.
- The shortcut legend starts on the same left edge as the composer.
- The tip uses left-only indentation and remains on one line at the reference width.
- Project context and version moved to the bottom edge, matching the reference hierarchy.

## Findings

- P0: none.
- P1: none.
- P2: none.
- P3: exact glyph weight and color rendering depend on the user's terminal font and color profile.

## Verification

- Real Konsole capture completed with true-color output enabled.
- Ratatui layout regression test completed at 116 columns by 40 rows.
- TUI unit tests and strict Clippy checks passed.

final result: passed

## Model selector scope

- Reference: Kimi Code model selector supplied by the user.
- Build under review: Auto Studio `/model` full-screen overlay.
- Compared state: connected `deepseek` catalog, current `deepseek-v4-pro`, selected Model Effort `Max`.
- Intentional product difference: Auto Studio currently has one active Provider Connection, so the selector shows the connected Provider tab instead of inventing an `All` catalog across disconnected Providers.

## Model selector checks

- Top and bottom accent borders, title, search affordance, navigation legend, cache/cost note, Provider tab, model rows, current marker and bottom effort switch preserve the reference hierarchy.
- Up/Down changes only the selected model row; Left/Right changes only Low / High / Max; both clamp at their boundaries.
- Opening `/model` restores the current model row and current effort instead of resetting to the first row.
- Enter emits one atomic model-plus-effort selection; Esc closes without a write.
- The selected model, connected Provider and current marker use distinct colors and remain readable with terminal true color enabled.
- Long catalogs use stateful list selection so the highlighted row stays visible when the terminal clips the list.

## Model selector findings

- P0: none.
- P1: none.
- P2: none.
- P3: font size, glyph weight and visible row count remain terminal-profile dependent.

## Model selector verification

- Real Konsole capture completed with `NO_COLOR` removed and true-color output enabled.
- Reference and implementation were resized to the same comparison height and reviewed side by side.
- Live terminal input proved Down changes `deepseek-v4-pro` to `deepseek-chat`, Left changes `Max` to `High`, and the inverse keys restore the original state.
- Ratatui buffer regression test verifies the full-screen hierarchy and selected `Max` state.
- Core/API/Provider/TUI workspace tests and strict Clippy checks are required before final handoff.

model selector final result: passed
