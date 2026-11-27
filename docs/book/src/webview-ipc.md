# Webview IPC and ACL

The webview hosts the DOM build in an OS window. The page can call native features, but only the ones
you grant.

Each grant is a `Capability`: an action name and the parameters it accepts. The grants are collected
into an `Acl`, which you pass to the app builder with `with_acl`. The builder also takes the window
configuration and a rate limit for the IPC bridge.

## How a message is handled

1. The page sends `{ action, params, requestId }` through `window.__appfront.post`, or through
   `window.ipc.postMessage`.
2. A message over the size limit is dropped before it is parsed.
3. The action must be in the ACL. Each parameter is checked against its `ParamSpec`: whether it is
   required, its type, and its default.
4. The rate limit applies to every message.
5. Built-in actions (dialogs, notifications, clipboard, secrets) run natively. Anything else goes to
   your handler.
6. A reply goes back to the page only when the message carried a `requestId`.

Keep the ACL short. Each capability hands the page a native power.

## Platform notes

- Windows and macOS use `wry`. Linux uses GTK 3 and WebKitGTK 4.1, loaded at runtime.
- On Linux there is one window per app, and no drag and drop or devtools. The
  [troubleshooting notes](https://github.com/tpt-solutions/tpt-appfront/blob/master/docs/troubleshooting.md)
  cover the details.
