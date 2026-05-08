---
name: ssh
description: Run a shell command on a remote SSH target through the tools-mcp `ssh_exec` MCP tool, optionally via SSH jump hosts.
argument-hint: "<COMMAND>" --host=... --user=... [--key-path=... | --password=...]
---

# /ssh

Run this shell command on the SSH target via the `ssh_exec` MCP tool:

```
$ARGUMENTS
```

## How to call it

1. **Parse the user's input.** First token is the command (often quoted).
   Required flags: `--host`, `--user`, and either `--password` or
   `--key-path`. Common shapes:
   - `/ssh "ls -la /var/log" --host=server.com --user=admin --key-path=~/.ssh/id_rsa`
   - `/ssh "df -h" --host=10.0.0.5 --user=root --password=...`
   - `/ssh "uname -a" --host=internal --user=admin --key-path=... --tunnel=ssh --ssh-jump=bastion --ssh-user=jumper --ssh-password=...`

2. **Translate into MCP tool params:** `command`, `host`, `port` (default 22),
   `user`, `password` OR `key_path` (mutually exclusive on the target).
   Plus the global tunnel/ssh_* fields when `--tunnel=ssh` is used —
   those are the JUMP credentials, separate from target creds.

3. **Call `ssh_exec`** with the params from Step 2.

4. **Render the result.** The response is an ExecutionResult with rows
   `["exit_code", "..."]`, `["stdout", "..."]`, `["stderr", "..."]`.
   - Show stdout to the user as text (markdown code block if it looks
     like structured output).
   - If `exit_code` is non-zero, mention it explicitly and show stderr.
   - If exit_code is 0 and stderr is non-empty, show stderr as a warning.

5. **Destructive commands** (anything that modifies state on the remote:
   `rm`, `mv`, `kill`, `systemctl restart`, `apt install`, `dd`, etc.):
   pause and confirm with the user BEFORE calling the tool. Especially
   if the command starts with `sudo` or runs as root.

## When something fails

- `Error::Config("SSH target requires --password or --key-path")` →
  the user supplied neither auth method.
- `Error::Connection("SSH connect to ... failed")` → can't reach the
  target on the SSH port. Check host/port; if going through a jump,
  the jump may be the problem (see ssh-bastion-checklist).
- `Error::Connection("SSH publickey/password auth failed")` → wrong
  creds. Note: TARGET creds are checked separately from JUMP creds.
- `Error::Service("SSH ...")` → russh-level error (channel open,
  exec request, etc.). Usually means the SSH session was terminated
  unexpectedly or the remote refused the channel.
- Commands needing a TTY (e.g. `top`, `htop`, `vim`) → fail because
  this Phase doesn't allocate a PTY. Run non-interactive variants
  (e.g. `top -bn1`) instead.
- SSH jump errors → use the **ssh-bastion-checklist** skill.
