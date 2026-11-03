# UI message flow

The launcher is an Iced Elm-style application. `view` only describes widgets;
`update` changes local state and sends search or execution requests through
Tokio mpsc channels without waiting for I/O.

The app's search worker receives each `SearchRequest` and calls
`SearchService::submit_query`. The UI subscribes to the shared
`tokio::sync::broadcast::Receiver<SearchEvent>`; result events become
`Message::ResultsReceived` and update the displayed rows, total count, and
elapsed time. Search failures are handled by the service's event broadcast and
can be surfaced by extending the UI status message flow.

Keyboard input enters through an Iced event subscription. It maps navigation,
execution, visibility, and numbered-selection shortcuts into the same message
loop as pointer interactions. App execution requests are sent over a second
Tokio mpsc channel for asynchronous handling by the composition root.

The window uses Iced's centered window position, which delegates monitor
selection and centering to the native window backend. Its style uses named
constants in `style` and the selected custom `Theme` palette.
