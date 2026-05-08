---
name: ssh-using
description: Use when calling the `ssh_exec` MCP tool from the tools-mcp plugin — explains target creds vs jump creds, output mapping (exit_code / stdout / stderr), PTY limitations, and common error shapes.
---

# Using the `ssh_exec` MCP tool

`tools-mcp` exposes an `ssh_exec` MCP tool. Runs one shell command on a target SSH server, returns exit_code + stdout + stderr in a flat ExecutionResult. Phase 7: no profile/YAML — just CLI/MCP fields.

## Tool input

```json
{
  "command":  "ls -la /var/log",
  "host":     "server.com",
  "port":     22,
  "user":     "admin",
  "password": "...",
  "key_path": "/home/me/.ssh/id_rsa",

  "tunnel":   "ssh",
  "ssh_jump": "bastion.com",
  "ssh_user": "jumper",
  "ssh_password": "...",
  "ssh_key_path": "/home/me/.ssh/jump_key",
  "ssh_port": 22
}
```

`command`, `host`, `user` are required. Either `password` OR `key_path` (mutually exclusive). Tunnel fields apply only when `tunnel = "ssh"`.

## Two credential sets

- **Target creds** (`user`, `password` / `key_path`, `port`) — for the SSH server where the command runs.
- **Jump creds** (`ssh_user`, `ssh_password` / `ssh_key_path`, `ssh_port`) — for the bastion(s) you go through to reach the target. ALL jumps share the same set.

If target and jump use the same credentials, you still need to supply both (the tool doesn't infer one from the other).

If `tunnel` is omitted or set to `"direct"`, no jumps are used and the target is reached directly.

## Output shape

ExecutionResult:

| field | value |
| --- | --- |
| `exit_code` | `0` |
| `stdout` | `total 12\ndrwxr-xr-x ...` |
| `stderr` | `` |

`exit_code = 0` means the command succeeded. Non-zero means it failed; show stderr to help the user understand why.

`<unknown>` for exit_code means the SSH channel closed without the remote sending an exit status — usually the connection died mid-execution. Treat as a failure.

Bytes are UTF-8-decoded if possible; binary stdout (rare for shell commands) renders as `<N bytes (non-UTF-8)>`.

## Common workflows

- **Check a service is up**: `systemctl status nginx` (exit_code 0 = active).
- **Tail a log**: `tail -n 100 /var/log/syslog` (then optionally pipe to local rg).
- **List process count**: `ps aux | wc -l`.
- **Disk free**: `df -h`.
- **Memory**: `free -h`.
- **Time on remote**: `date -u`.

For LARGER tasks (multi-minute), `ssh_exec` is fine — it waits for the channel to close. There's no streaming; the tool returns when the remote closes stdout.

## PTY / TTY limitation

`ssh_exec` does NOT allocate a PTY. Commands that require a TTY (e.g. `top`, `htop`, `vim`, `passwd`, anything calling `isatty(stdin)`) will fail or behave unexpectedly. Use non-interactive variants (`top -bn1`, etc.) or run the command via `bash -c '...'` if shell features are needed.

## Destructive commands

Confirm with the user BEFORE running:
- `rm`, `unlink`, `find ... -delete`
- `mv` to overwrite paths
- `dd` writing to disk
- `mkfs.*`
- `systemctl restart` / `reboot` / `shutdown`
- `apt install` / `apt remove` / `pip install` / etc.
- `kill -9`, `pkill`
- Anything starting with `sudo` (escalation)

Read-only commands (`ls`, `cat`, `grep`, `df`, `free`, `ps`, `systemctl status`, `journalctl`, `dmesg`, `find ... -type f`) are safe to run without a confirmation prompt.

## Common error shapes

- `Error::Config("SSH target requires --password or --key-path")` — neither auth method supplied.
- `Error::Config("password and key_path are mutually exclusive")` — both supplied for the target.
- `Error::Connection("SSH connect to ... failed")` — can't reach the target SSH port.
- `Error::Connection("SSH publickey/password auth failed")` — wrong target credentials. (Note: jump auth uses different fields, so a wrong password here doesn't affect the jump chain.)
- `Error::Connection("SSH password authentication rejected")` — the password fallback (after publickey) was rejected by the server.
- `Error::Service("SSH session open failed")` — russh couldn't open a session channel on the target. Usually means the server killed the connection.
- `Error::Service("SSH exec request failed")` — server refused the exec request (rare; possibly forced-command setup or a restricted shell).
- SSH jump errors → use the **ssh-bastion-checklist** skill.

## What this skill is NOT

- Not for SCP/SFTP file transfer (Phase 8+).
- Not for interactive shells / PTY-required commands.
- Not for `mysql_exec` / `redis_exec` / `http_exec` — see the respective skills.
