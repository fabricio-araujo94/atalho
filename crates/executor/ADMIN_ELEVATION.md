# Administrator Elevation

Entries discovered from Windows shortcuts retain the original `.lnk` path and
the shortcut's run-as flag. The executor opens shortcuts with `ShellExecuteW`,
using the `runas` verb when elevation is requested; this preserves shortcut
settings such as the working directory and target arguments. Explicitly
elevated non-shortcut entries also use `ShellExecuteW` with `runas`.

Ordinary executables are launched with `CreateProcessW` and `DETACHED_PROCESS`.
Windows does not support requesting UAC elevation through that launch path, so
entries marked as requiring elevation must use the ShellExecute path. UAC may
show a consent prompt, and the user can decline it; these failures are returned
to the UI as transient launch errors. A shortcut's run-as flag indicates that
Windows should request elevation, but does not guarantee that the application
will successfully start with elevated privileges.
