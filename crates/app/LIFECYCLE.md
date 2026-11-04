# Launcher lifecycle

The process acquires `Global\FlowLauncherRust.SingleInstance` before starting
the UI or tray. A second process signals the primary instance through the
named show event and exits. The primary instance creates the tray notification
icon and runs its Win32 message pump on a dedicated thread; tray actions are
forwarded to Iced over an asynchronous command channel.

The tray menu provides **Show**, **Settings**, and **Quit**. Left-click and
double-click toggle launcher visibility. Settings exposes the per-user
sign-in startup option, which creates or removes
`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\FlowLauncherRust.lnk`
and persists `auto_start` in the application configuration.

On quit, the app cancels active search and re-index operations, stops its
request workers, writes the current index, flushes launch history, and saves
configuration. It then unregisters manager-owned hotkeys, stops the
single-instance signal listener, removes the tray icon, releases the mutex,
and exits. Cleanup is best effort: an individual failure is logged and does
not prevent later shutdown steps. The tray pump and startup shortcut COM calls
run outside the UI and async-runtime threads.
