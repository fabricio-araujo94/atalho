//! Named launcher dimensions and palette constants.

use iced::Color;

/// Initial window width in logical pixels.
pub const WINDOW_WIDTH: f32 = 680.0;
/// Initial window height in logical pixels.
pub const INITIAL_WINDOW_HEIGHT: f32 = 64.0;
/// Maximum launcher height when displaying results.
pub const MAX_WINDOW_HEIGHT: f32 = 640.0;
/// Maximum result rows included in the initial response window expansion.
pub const MAX_VISIBLE_RESULTS: u16 = 16;
/// Height reserved for each visible result.
pub const RESULT_ROW_HEIGHT: f32 = 34.0;
/// Launcher panel padding.
pub const PANEL_PADDING: f32 = 12.0;
/// Vertical spacing between search, results, and status sections.
pub const SECTION_SPACING: f32 = 8.0;
/// Horizontal spacing between items in a row.
pub const ROW_SPACING: f32 = 10.0;
/// Vertical gap between result rows.
pub const RESULT_SPACING: f32 = 2.0;
/// Padding around each selectable result.
pub const RESULT_ROW_PADDING: f32 = 7.0;
/// Padding around the search field.
pub const SEARCH_PADDING: f32 = 6.0;
/// Padding around clear and action buttons.
pub const BUTTON_PADDING: f32 = 6.0;
/// Result icon and search icon dimensions.
pub const ICON_SIZE: f32 = 16.0;
/// Fixed width reserved for result icons.
pub const ICON_COLUMN_WIDTH: f32 = 20.0;
/// Search text size.
pub const SEARCH_TEXT_SIZE: f32 = 22.0;
/// Result name text size.
pub const RESULT_TEXT_SIZE: f32 = 16.0;
/// Source badge text size.
pub const BADGE_TEXT_SIZE: f32 = 11.0;
/// Status text size.
pub const STATUS_TEXT_SIZE: f32 = 11.0;
/// Gap between status labels.
pub const STATUS_SPACING: f32 = 16.0;
/// Rounded panel corner radius.
pub const CORNER_RADIUS: f32 = 8.0;
/// Drop shadow blur radius.
pub const SHADOW_BLUR: f32 = 18.0;
/// Drop shadow offset.
pub const SHADOW_OFFSET: f32 = 2.0;
/// Event stream buffer size.
pub const EVENT_CHANNEL_CAPACITY: usize = 32;
/// Search placeholder text.
pub const SEARCH_PLACEHOLDER: &str = "Search applications...";
/// Magnifier glyph used in the search bar.
pub const MAGNIFIER: &str = "⌕";
/// Clear button glyph.
pub const CLEAR_GLYPH: &str = "×";
/// Generic application icon glyph.
pub const APP_ICON: &str = "▣";
/// Shadow color.
pub const SHADOW_COLOR: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.35);

/// Dark theme background.
pub const DARK_BACKGROUND: Color = Color::from_rgb(0.10, 0.11, 0.14);
/// Dark theme foreground.
pub const DARK_FOREGROUND: Color = Color::from_rgb(0.92, 0.93, 0.96);
/// Dark theme accent.
pub const DARK_ACCENT: Color = Color::from_rgb(0.45, 0.68, 1.0);
/// Dark theme selected result.
pub const DARK_SELECTION: Color = Color::from_rgb(0.20, 0.25, 0.34);
/// Dark theme match highlight.
pub const DARK_HIGHLIGHT: Color = Color::from_rgb(1.0, 0.78, 0.35);

/// Light theme background.
pub const LIGHT_BACKGROUND: Color = Color::from_rgb(0.97, 0.97, 0.98);
/// Light theme foreground.
pub const LIGHT_FOREGROUND: Color = Color::from_rgb(0.12, 0.13, 0.16);
/// Light theme accent.
pub const LIGHT_ACCENT: Color = Color::from_rgb(0.12, 0.38, 0.75);
/// Light theme selected result.
pub const LIGHT_SELECTION: Color = Color::from_rgb(0.84, 0.89, 0.97);
/// Light theme match highlight.
pub const LIGHT_HIGHLIGHT: Color = Color::from_rgb(0.65, 0.35, 0.0);
