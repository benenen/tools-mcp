---
name: ssh-bastion-checklist
description: Use when `mysql_exec` (or any tools4a call with `tunnel=ssh`) fails at the SSH layer — connect failure, auth failure, direct-tcpip channel failure, or a multi-hop chain that drops between hops. Walks through diagnostic checks before changing the call.
---

# SSH bastion / tunnel troubleshooting checklist

`tools4a`'s SSH tunnel walks the chain `client → bastion1 → … → bastionN → target_host`. A failure in any of those edges produces a different error message. Pick the matching section, run the listed checks, then propose a fix.

## Map error message → likely cause

| Substring in error | Stage | Section |
|---|---|---|
| `SSH connect to <host> failed` | Client → first bastion | A: TCP / first hop |
| `SSH publickey auth failed` / `SSH password auth failed` / `SSH password authentication rejected` | Auth on a bastion | B: Auth |
| `SSH authentication failed: no usable credentials` | Caller didn't supply auth | B: Auth |
| `open direct-tcpip to ... via prior hop failed` | Inside-tunnel hop (bastion → next bastion or target) | C: Inside-tunnel |
| `SSH connect to <host> (chained) failed` | Second SSH handshake over a direct-tcpip channel | C: Inside-tunnel |
| Hangs with no output | Unresponsive bastion / firewall drop | D: Hang |

The host-key warning (`warning: accepting unverified host key for ...`) on stderr is **not** an error — Phase 2/3 accepts any host key by design. Ignore it.

## A: TCP / first hop fails

The first hop is a plain TCP connection from your machine to the bastion. Check from the client:

1. **Is the host reachable?** `nc -zv <bastion> <ssh_port>` (or `nc -vz` on macOS). Expect `succeeded`.
2. **VPN required?** If the bastion is on an internal network and you can't reach it, you may need to be on a corporate VPN.
3. **Right port?** Default is 22; some bastions use 2222/2200/22000. Check `--ssh-port` / `ssh_port` matches.
4. **DNS vs IP?** Try the IP directly. If the IP works but the hostname doesn't, it's DNS.

## B: Auth fails

Auth happens once per hop. All hops share `ssh_user` / `ssh_password` / `ssh_key_path`.

1. **Verify creds work via plain `ssh`**: `ssh -p <port> <ssh_user>@<bastion>`. If this also fails, the credentials are wrong.
2. **If using `ssh_key_path`**: file permissions must be `600` (or `400`). `chmod 600 ~/.ssh/id_rsa`. Also: passphrase-protected keys are NOT supported in Phase 2/3 — convert to an unencrypted key (`ssh-keygen -p -f keyfile`) or use password auth.
3. **If using `ssh_password`**: bastion may have password auth disabled (`PasswordAuthentication no` in sshd_config). Switch to key auth.
4. **Both supplied**: tools4a tries publickey first, then falls back to password. If publickey fails for any reason (bad key file, permissions, key not in `authorized_keys`), the password attempt happens next — so a `password authentication rejected` error means the FALLBACK also failed.
5. **Per-hop auth difference**: not supported yet. If bastion1 and bastion2 need different credentials, this Phase can't reach the target. Phase 3+ may add per-hop overrides; for now, consolidate so all hops accept the same credential.

## C: Inside-tunnel failure (`direct-tcpip ... failed` / `chained ... failed`)

Means the previous hop is up and authenticated, but couldn't open the channel to the next host.

1. **From the bastion** (`ssh <bastion>` then run): `nc -zv <next_host> <port>`. Verifies the bastion can actually reach the next host.
2. **Firewall on bastion?** AWS / GCP security groups, on-host iptables. Bastions are often locked down to "only outbound to specific subnets".
3. **Wrong target host?** If the target is internal, it might only resolve from inside the network. Try the IP directly via `--host` / `host`.
4. **Wrong target port?** Common typo: `3306` (MySQL) vs `5432` (Postgres) vs `6379` (Redis).
5. **For multi-hop**: if hop1→hop2 fails, hop1 needs network reach to hop2 on `ssh_port`. Same checks as the first-hop section, but run from inside the bastion.

## D: Hangs

If `mysql_exec` doesn't return within ~30s and there's no error:

1. The bastion might be unresponsive — check `nc -zv` from the client.
2. Some bastions have `MaxStartups`/`MaxSessions` limits and silently drop new connections under load. Try again, or check with the operator.
3. Confirm the tunnel actually completes: kill the call, then run the same params via `cargo run -- --tunnel=ssh ...` from the CLI. The CLI prints intermediate progress to stderr; the MCP path doesn't.

## Quick self-test

To verify the tools4a tunnel itself works (separating "tunnel broken" from "MySQL broken"):

```bash
tools4a --tunnel=ssh --ssh-jump=<bastion> --ssh-user=<user> --ssh-password=<pwd> \
  mysql --host=127.0.0.1 --port=22 --user=anything --password=anything 'select 1'
```

If you get `SSH publickey/password auth failed` for the bastion → auth issue (B).
If you get a MySQL connection error to `127.0.0.1:22` → tunnel itself is up (the test target is intentionally bogus). The problem is then with the real target host/port (C).

## What this skill is NOT

- Not generic SSH client troubleshooting (look at `ssh -vv` for that).
- Not for problems WITHIN the database (use `mysql-debugging` once the tunnel is up).
