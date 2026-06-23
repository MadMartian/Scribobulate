# Unsafe-scheme image fixture

These reference non-http(s) URI schemes. Even with Show-Unsafe-Images ON, only
`http`/`https` are admitted — these must stay refused (§14.8):

A `file://` scheme image:

![local file scheme](file:///etc/hostname.png)

An `smb://` scheme image:

![smb share image](smb://example/share/pic.png)

Body text after, to confirm surrounding prose still renders.
