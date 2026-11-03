//! Iced-based graphical interface for the Atalho launcher.

pub mod style;

use core::{AppEntry, ScoredMatch, SearchEvent, SearchRequest};
use iced::futures::SinkExt;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{
    Alignment, Color, Element, Event, Fill, Length, Subscription, Task, Theme as IcedTheme, event,
    window,
};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, broadcast, mpsc};

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);

/// Custom launcher palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Main panel fill.
    pub background: Color,
    /// Primary label color.
    pub foreground: Color,
    /// Interactive accent.
    pub accent: Color,
    /// Selected result fill.
    pub selection: Color,
    /// Match highlight color.
    pub highlight: Color,
}

impl Theme {
    /// Dark launcher palette.
    pub const DARK: Self = Self {
        background: style::DARK_BACKGROUND,
        foreground: style::DARK_FOREGROUND,
        accent: style::DARK_ACCENT,
        selection: style::DARK_SELECTION,
        highlight: style::DARK_HIGHLIGHT,
    };

    /// Light launcher palette.
    pub const LIGHT: Self = Self {
        background: style::LIGHT_BACKGROUND,
        foreground: style::LIGHT_FOREGROUND,
        accent: style::LIGHT_ACCENT,
        selection: style::LIGHT_SELECTION,
        highlight: style::LIGHT_HIGHLIGHT,
    };
}

impl Default for Theme {
    fn default() -> Self {
        Self::DARK
    }
}

/// Messages handled by the launcher Elm update loop.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Search input changed.
    QueryChanged(String),
    /// Search service returned a result set.
    ResultsReceived(Vec<ScoredMatch>, usize, u64),
    /// Search service started processing the latest query.
    SearchLoading,
    /// Search service reported an error.
    SearchFailed(String),
    /// Move selection to the previous result.
    SelectionUp,
    /// Move selection to the next result.
    SelectionDown,
    /// Execute the selected result.
    ExecuteSelected,
    /// Show or hide the launcher contents.
    ToggleVisibility,
    /// Switch the launcher palette.
    ThemeChanged(Theme),
    /// Clear the current query.
    ClearQuery,
    /// Select a result row by index.
    SelectResult(usize),
    /// Execute a result by index.
    ExecuteResult(usize),
}

/// Iced application state for the launcher.
pub struct LauncherState {
    /// Current search text.
    pub query: String,
    /// Ranked result candidates.
    pub results: Vec<ScoredMatch>,
    /// Selected result index.
    pub selected_index: usize,
    /// Whether launcher content is currently visible.
    pub is_visible: bool,
    /// Active launcher palette.
    pub theme: Theme,
    /// Maximum number of displayed search results.
    pub max_results: usize,
    /// Total number of matches before truncation.
    pub total_results: usize,
    /// Last search duration in microseconds.
    pub elapsed_us: u64,
    /// Most recent search error, if any.
    pub search_error: Option<String>,
    query_sender: mpsc::UnboundedSender<SearchRequest>,
    execute_sender: mpsc::UnboundedSender<AppEntry>,
    search_events: SearchEventSource,
}

impl LauncherState {
    /// Creates launcher state connected to the async search and execution channels.
    #[must_use]
    pub fn new(
        query_sender: mpsc::UnboundedSender<SearchRequest>,
        execute_sender: mpsc::UnboundedSender<AppEntry>,
        search_events: SearchEventSource,
        max_results: usize,
    ) -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            is_visible: true,
            theme: Theme::default(),
            max_results,
            total_results: 0,
            elapsed_us: 0,
            search_error: None,
            query_sender,
            execute_sender,
            search_events,
        }
    }

    /// Updates state and sends side-effectful work through async channels.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueryChanged(query) => {
                self.query.clone_from(&query);
                self.selected_index = 0;
                if let Err(error) = self
                    .query_sender
                    .send(SearchRequest::new(query, self.max_results))
                {
                    self.search_error = Some(format!("Search service unavailable: {error}"));
                }
            }
            Message::ResultsReceived(results, total, elapsed_us) => {
                self.results = results.into_iter().take(self.max_results).collect();
                self.total_results = total;
                self.elapsed_us = elapsed_us;
                self.search_error = None;
                self.selected_index = 0;
                let visible_rows = u16::try_from(self.results.len())
                    .unwrap_or(u16::MAX)
                    .min(style::MAX_VISIBLE_RESULTS);
                let new_size = iced::Size::new(
                    style::WINDOW_WIDTH,
                    style::RESULT_ROW_HEIGHT
                        .mul_add(f32::from(visible_rows), style::INITIAL_WINDOW_HEIGHT),
                );
                return window::latest()
                    .then(move |id| id.map_or_else(Task::none, |id| window::resize(id, new_size)));
            }
            Message::SearchLoading => self.search_error = None,
            Message::SearchFailed(error) => self.search_error = Some(error),
            Message::SelectionUp => self.select_previous(),
            Message::SelectionDown => self.select_next(),
            Message::ExecuteSelected => self.execute_selected(),
            Message::ToggleVisibility => {
                self.is_visible = !self.is_visible;
                let minimized = !self.is_visible;
                return window::latest().then(move |id| {
                    id.map_or_else(Task::none, |id| window::minimize(id, minimized))
                });
            }
            Message::ThemeChanged(theme) => self.theme = theme,
            Message::ClearQuery => {
                self.query.clear();
                self.selected_index = 0;
                if let Err(error) = self
                    .query_sender
                    .send(SearchRequest::new(String::new(), self.max_results))
                {
                    self.search_error = Some(format!("Search service unavailable: {error}"));
                }
            }
            Message::SelectResult(index) => {
                if index < self.results.len() {
                    self.selected_index = index;
                }
            }
            Message::ExecuteResult(index) => self.execute_at(index),
        }
        Task::none()
    }

    const fn select_previous(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.results.len() - 1
            } else {
                self.selected_index - 1
            };
        }
    }

    const fn select_next(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.results.len();
        }
    }

    fn execute_selected(&mut self) {
        self.execute_at(self.selected_index);
    }

    fn execute_at(&mut self, index: usize) {
        if let Some(entry) = self.results.get(index).map(|result| result.entry.clone())
            && let Err(error) = self.execute_sender.send(entry)
        {
            self.search_error = Some(format!("Application executor unavailable: {error}"));
        }
    }
}

/// Cloneable handle that carries search broadcast events into an Iced subscription.
#[derive(Clone)]
pub struct SearchEventSource(EventSubscription);

impl SearchEventSource {
    /// Wraps a broadcast receiver for use by a launcher instance.
    #[must_use]
    pub fn new(receiver: broadcast::Receiver<SearchEvent>) -> Self {
        Self(EventSubscription {
            id: NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed),
            receiver: Arc::new(Mutex::new(receiver)),
        })
    }
}

/// Runs the Iced launcher with a 680×64 borderless, transparent, topmost window.
///
/// The application should drain the query and execution channels on its async
/// runtime, forwarding query requests to `SearchService::submit_query`.
///
/// # Errors
/// Returns an error if Iced cannot create or run the native launcher window.
pub fn run(
    query_sender: mpsc::UnboundedSender<SearchRequest>,
    execute_sender: mpsc::UnboundedSender<AppEntry>,
    search_events: broadcast::Receiver<SearchEvent>,
    max_results: usize,
) -> iced::Result {
    let search_events = SearchEventSource::new(search_events);
    iced::application(
        move || {
            LauncherState::new(
                query_sender.clone(),
                execute_sender.clone(),
                search_events.clone(),
                max_results,
            )
        },
        |state: &mut LauncherState, message| state.update(message),
        view,
    )
    .subscription(subscription)
    .theme(|_: &LauncherState| IcedTheme::Dark)
    .style(
        |state: &LauncherState, _theme: &IcedTheme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: state.theme.foreground,
        },
    )
    .window(window::Settings {
        size: iced::Size::new(style::WINDOW_WIDTH, style::INITIAL_WINDOW_HEIGHT),
        decorations: false,
        transparent: true,
        resizable: false,
        minimizable: false,
        closeable: false,
        level: window::Level::AlwaysOnTop,
        min_size: Some(iced::Size::new(
            style::WINDOW_WIDTH,
            style::INITIAL_WINDOW_HEIGHT,
        )),
        max_size: Some(iced::Size::new(
            style::WINDOW_WIDTH,
            style::MAX_WINDOW_HEIGHT,
        )),
        position: window::Position::SpecificWith(centered_position),
        exit_on_close_request: false,
        ..window::Settings::default()
    })
    .run()
}

fn centered_position(window_size: iced::Size, monitor_size: iced::Size) -> iced::Point {
    let (x, y) = platform::centered_window_origin(
        (window_size.width, window_size.height),
        (monitor_size.width, monitor_size.height),
    );
    iced::Point::new(x, y)
}

/// Pure launcher view.
pub fn view(state: &LauncherState) -> Element<'_, Message> {
    if !state.is_visible {
        return container(text("")).width(Fill).height(Fill).into();
    }

    let search_bar = row![
        text(style::MAGNIFIER).size(style::ICON_SIZE),
        text_input(style::SEARCH_PLACEHOLDER, &state.query)
            .on_input(Message::QueryChanged)
            .size(style::SEARCH_TEXT_SIZE)
            .padding(style::SEARCH_PADDING)
            .width(Fill),
        button(text(style::CLEAR_GLYPH).size(style::ICON_SIZE))
            .on_press(Message::ClearQuery)
            .padding(style::BUTTON_PADDING),
    ]
    .spacing(style::ROW_SPACING)
    .align_y(Alignment::Center);

    let mut result_rows = column![].spacing(style::RESULT_SPACING).width(Fill);
    for (index, scored) in state.results.iter().take(state.max_results).enumerate() {
        let selected = index == state.selected_index;
        let entry = &scored.entry;
        let row_contents = row![
            text(style::APP_ICON)
                .size(style::ICON_SIZE)
                .width(style::ICON_COLUMN_WIDTH),
            text(&entry.name)
                .size(style::RESULT_TEXT_SIZE)
                .font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::default()
                })
                .color(if scored.match_positions.is_empty() {
                    state.theme.foreground
                } else {
                    state.theme.highlight
                })
                .width(Fill),
            text(source_badge(entry.source))
                .size(style::BADGE_TEXT_SIZE)
                .color(state.theme.accent),
        ]
        .spacing(style::ROW_SPACING)
        .align_y(Alignment::Center);
        result_rows = result_rows.push(
            button(row_contents)
                .on_press(Message::SelectResult(index))
                .width(Fill)
                .padding(style::RESULT_ROW_PADDING)
                .style(move |_theme, status| {
                    let background = if selected || status == button::Status::Hovered {
                        state.theme.selection
                    } else {
                        Color::TRANSPARENT
                    };
                    button::Style {
                        background: Some(background.into()),
                        text_color: state.theme.foreground,
                        border: iced::Border::default().rounded(style::CORNER_RADIUS),
                        ..button::Style::default()
                    }
                }),
        );
    }
    let results = scrollable(result_rows).height(Length::Shrink).width(Fill);

    let status = row![
        text(state.search_error.as_deref().map_or_else(
            || format!("{} results", state.total_results),
            std::borrow::ToOwned::to_owned,
        ))
        .size(style::STATUS_TEXT_SIZE)
        .color(
            state
                .search_error
                .as_ref()
                .map_or(state.theme.foreground, |_| { state.theme.highlight })
        ),
        text(format!("{} μs", state.elapsed_us)).size(style::STATUS_TEXT_SIZE),
    ]
    .spacing(style::STATUS_SPACING)
    .align_y(Alignment::Center);

    container(
        column![search_bar, results, status]
            .spacing(style::SECTION_SPACING)
            .padding(style::PANEL_PADDING)
            .width(Fill),
    )
    .width(Fill)
    .style(move |_theme| iced::widget::container::Style {
        background: Some(state.theme.background.into()),
        text_color: Some(state.theme.foreground),
        border: iced::Border::default().rounded(style::CORNER_RADIUS),
        shadow: iced::Shadow {
            color: style::SHADOW_COLOR,
            offset: iced::Vector::new(style::SHADOW_OFFSET, style::SHADOW_OFFSET),
            blur_radius: style::SHADOW_BLUR,
        },
        ..iced::widget::container::Style::default()
    })
    .into()
}

const fn source_badge(source: core::AppSource) -> &'static str {
    match source {
        core::AppSource::StartMenu => "START",
        core::AppSource::Path => "PATH",
        core::AppSource::Desktop => "DESKTOP",
        core::AppSource::UserPinned => "PINNED",
    }
}

fn subscription(state: &LauncherState) -> Subscription<Message> {
    Subscription::batch([
        event::listen_with(keyboard_message),
        Subscription::run_with(state.search_events.0.clone(), search_event_stream),
    ])
}

fn keyboard_message(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    map_key(key, modifiers)
}

fn map_key(key: Key, modifiers: Modifiers) -> Option<Message> {
    if modifiers.control() {
        if let Key::Character(character) = key
            && let Some(number) = character.as_str().parse::<usize>().ok()
            && (1..=9).contains(&number)
        {
            return Some(Message::ExecuteResult(number - 1));
        }
        return None;
    }
    match key {
        Key::Named(keyboard::key::Named::ArrowUp) => Some(Message::SelectionUp),
        Key::Named(keyboard::key::Named::ArrowDown | keyboard::key::Named::Tab) => {
            Some(Message::SelectionDown)
        }
        Key::Named(keyboard::key::Named::Enter) => Some(Message::ExecuteSelected),
        Key::Named(keyboard::key::Named::Escape) => Some(Message::ToggleVisibility),
        _ => None,
    }
}

#[derive(Clone)]
struct EventSubscription {
    id: u64,
    receiver: Arc<Mutex<broadcast::Receiver<SearchEvent>>>,
}

impl PartialEq for EventSubscription {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for EventSubscription {}

impl Hash for EventSubscription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

fn search_event_stream(
    subscription: &EventSubscription,
) -> impl iced::futures::Stream<Item = Message> + 'static + use<> {
    let receiver = Arc::clone(&subscription.receiver);
    iced::stream::channel(style::EVENT_CHANNEL_CAPACITY, async move |mut output| {
        loop {
            let event = receiver.lock().await.recv().await;
            match event {
                Ok(SearchEvent::Results(response)) => {
                    let message = Message::ResultsReceived(
                        response.results,
                        response.total,
                        response.elapsed_us,
                    );
                    if output.send(message).await.is_err() {
                        break;
                    }
                }
                Ok(SearchEvent::Loading) => {
                    if output.send(Message::SearchLoading).await.is_err() {
                        break;
                    }
                }
                Ok(SearchEvent::Error(error)) => {
                    if output
                        .send(Message::SearchFailed(error.to_string()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{AppSource, ScoredMatch};

    fn state() -> LauncherState {
        let (query_sender, _) = mpsc::unbounded_channel();
        let (execute_sender, _) = mpsc::unbounded_channel();
        let (event_sender, event_receiver) = broadcast::channel(4);
        drop(event_sender);
        LauncherState::new(
            query_sender,
            execute_sender,
            SearchEventSource::new(event_receiver),
            8,
        )
    }

    #[test]
    fn query_update_emits_search_request() {
        let (query_sender, mut query_receiver) = mpsc::unbounded_channel();
        let (execute_sender, _) = mpsc::unbounded_channel();
        let (event_sender, event_receiver) = broadcast::channel(4);
        let mut state = LauncherState::new(
            query_sender,
            execute_sender,
            SearchEventSource::new(event_receiver),
            8,
        );

        let _task = state.update(Message::QueryChanged("calc".to_owned()));

        assert_eq!(
            query_receiver.try_recv().ok(),
            Some(SearchRequest::new("calc", 8))
        );
        drop(event_sender);
    }

    #[test]
    fn view_builds_with_results_and_keyboard_navigation_wraps() {
        let mut state = state();
        state.results = vec![
            ScoredMatch::new(
                AppEntry::new("First", "first.exe", AppSource::StartMenu),
                20,
                vec![0],
            ),
            ScoredMatch::new(
                AppEntry::new("Second", "second.exe", AppSource::Path),
                10,
                Vec::new(),
            ),
        ];
        let element = view(&state);
        drop(element);

        let _task = state.update(Message::SelectionUp);
        assert_eq!(state.selected_index, 1);
        let _task = state.update(Message::SelectionDown);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn ctrl_digit_selects_requested_result_and_execute_message_is_distinct() {
        let mapped = map_key(Key::Character("3".into()), Modifiers::CTRL);
        assert_eq!(mapped, Some(Message::ExecuteResult(2)));
        assert_eq!(
            map_key(
                Key::Named(keyboard::key::Named::Enter),
                Modifiers::default()
            ),
            Some(Message::ExecuteSelected)
        );
    }
}
