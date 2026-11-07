# Remote access — use the big Mac from a smaller one

One Mac holds the models and does the work (the **server**); other Macs on the
same network, or on the home VPN, use it (the **clients**). What travels is a
prompt, a page of ComfyUI and the finished images, never the weights — they stay
in the server's `~/ai-models/`, and so does every render.

**Nothing is offered to the network.** ComfyUI and Ollama keep their default
loopback binding, so only the server itself can reach them. The one open port is
SSH, it takes keys only, and each client's key can open exactly two tunnels — to
ComfyUI and to Ollama — and nothing else: no shell, no other port. That holds on
any network either machine is on, which is the point: a firewall rule at home is
a second layer here, not the only one.

```
client                                          server
  browser ──► localhost:8188  ──┐            ┌──► 127.0.0.1:8188   ComfyUI
  enhance.sh ► localhost:21434 ─┴── ssh :22 ─┴──► 127.0.0.1:11434  Ollama
```

Covered: **ComfyUI** in the client's browser, and **prompt enhancement**
(`enhance.sh`, the app's **Enhance**). Not covered: `generate.sh` and the app's
**Generate** run mflux on the machine they are started on — on a client, that is
the client.

## Server

### 1. Remote Login, keys only

System Settings → General → Sharing → **Remote Login** on; under its ⓘ, allow
only your own user.

macOS ships with password logins **on**: a client without a key is refused with
`Permission denied (publickey,password,keyboard-interactive)` — all three methods
still on offer. Turn passwords off, and reverse forwarding with them (see step 3
for why):

```bash
sudo tee /etc/ssh/sshd_config.d/050-imagegen.conf >/dev/null <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
AllowTcpForwarding local
EOF
sudo sshd -t
sudo sshd -T | grep -Ei '^(passwordauthentication|kbdinteractiveauthentication|allowtcpforwarding) '
```

The last line must print `no`, `no` and `local`. sshd keeps the **first** value
it reads, and `/etc/ssh/sshd_config` includes `sshd_config.d/*` in name order at
its very top — hence `050-`, ahead of Apple's own `100-macos.conf`. macOS starts
sshd per connection, so there is nothing to restart: the next login uses the new
file. From a client, the refusal now names `publickey` alone —
[Check it](#check-it), first test.

`AllowTcpForwarding local` applies to every key on the server, yours included:
nobody can `ssh -R` onto this Mac any more. `-L` tunnels are unaffected.

### 2. Keep the services on loopback

- **ComfyUI** — start it plain, `./comfyui-start.sh`. Its default is
  `127.0.0.1` (`comfyui/comfy/cli_args.py`). Never add `--listen`: on its own,
  without an address, it means *every* interface (`0.0.0.0,::`).
- **Ollama** — its default is `127.0.0.1:11434`. Do not set `OLLAMA_HOST` for
  the server; if it was set for the menu-bar app, `launchctl unsetenv OLLAMA_HOST`
  and restart the app. Pull the model `enhance.sh` uses (`AI_ENHANCE_MODEL` in
  `env.sh`) here — the clients' copies are not involved.

With both running:

```bash
lsof -nP -iTCP -sTCP:LISTEN | grep -E ':(8188|11434) '
```

Every line must say `127.0.0.1:`. A `*:` or a LAN address means that service is
on the network.

### 3. Let a client in

One line per client in `~/.ssh/authorized_keys` (directory `700`, file `600`),
with that client's public key from below:

```
restrict,port-forwarding,permitopen="127.0.0.1:8188",permitopen="127.0.0.1:11434",command="/usr/bin/false" ssh-ed25519 AAAA… laptop-imagegen
```

| Option | Does |
|---|---|
| `restrict` | turns off everything: forwarding, agent, X11, terminal |
| `port-forwarding` | turns tunnels back on… |
| `permitopen="…"` ×2 | …to these two destinations only |
| `command="/usr/bin/false"` | any command, a shell included, exits at once |

To lock a client out, delete its line. Use a key made for this, never the
client's everyday key: if that one is also in the file unrestricted, the limits
on this line mean nothing.

Two things that cost time in testing:

- `permitopen` compares the **literal** host the client asks for. A tunnel to
  `localhost:8188` is refused (`administratively prohibited`); it has to say
  `127.0.0.1:8188`.
- `port-forwarding` turns `-R` (a listener on the server's loopback, which could
  pose as Ollama while the real one is down) back on along with `-L`. There is no
  per-key way to switch it off: `permitlisten="none"` is not valid, and sshd then
  ignores the **whole** line — the key stops working. Hence `AllowTcpForwarding
  local` in step 1.

## Client

### 1. A key for this

```bash
ssh-keygen -t ed25519 -f ~/.ssh/imagegen -C "$(scutil --get LocalHostName)-imagegen"
```

Give it a passphrase and load it into your agent like your other keys. Get
`~/.ssh/imagegen.pub` onto the server by AirDrop or copy-paste, and put it into
the line from server step 3. `ssh-copy-id` cannot do this once passwords are off,
and would add the key unrestricted anyway.

### 2. One alias in `~/.ssh/config`

```
Host imagegen
    HostName <server-address>
    User <your-user-on-the-server>
    IdentityFile ~/.ssh/imagegen
    IdentitiesOnly yes
    SessionType none
    ExitOnForwardFailure yes
    ServerAliveInterval 30
    LocalForward 8188 127.0.0.1:8188
    LocalForward 21434 127.0.0.1:11434
```

- `HostName` is the server's **LAN IP**, not a `.local` name: those are resolved
  by multicast on the local network and do not cross a VPN. Reserve that IP for
  the server in the router's DHCP so it stays put.
- `SessionType none` is `ssh -N`: tunnels, no command.
- `21434`, not `11434`, because a client that has Ollama installed is already
  listening on `11434` itself. `ExitOnForwardFailure` makes a taken port an
  error at connect time instead of a silently missing tunnel.

### 3. Use it

At home, or from outside with the home VPN connected — the alias is the same:

```bash
ssh imagegen        # holds the tunnels until Ctrl-C; keep it in its own tab
```

The first time, ssh shows the server's host-key fingerprint and asks. Answer
`yes` only if it matches what the server itself reports:

```bash
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub     # on the server
```

That comparison is what stops a machine on a foreign network, or on the way
through it, from posing as the server.

- **ComfyUI:** open `http://localhost:8188`. Renders are saved on the server, in
  its `comfyui/output/`; download them from the browser.
- **Enhance:**

  ```bash
  AI_ENHANCE_HOST=http://localhost:21434 AI_ENHANCE_KEEP=0 ./enhance.sh "a dragon"
  ```

  For the app's **Enhance** button, start the app from a shell with those two
  variables exported (`cargo run --release` in `gui/`).

`AI_ENHANCE_KEEP=0` matters on a shared server. `enhance.sh` otherwise asks
Ollama to keep the model resident for five minutes, ~31 GB of it, and only
`generate.sh` on the server unloads it before a render — ComfyUI does not. A
FLUX.2 [dev] render on top of that swaps. The price is a model load (~20 s) per
enhance.

## Check it

From the client, after the first `ssh imagegen` (until the host key is known,
the first test fails on `Host key verification failed` instead):

```bash
# 1. passwords are off — expect: Permission denied (publickey).
#    before step 1 on the server: (publickey,password,keyboard-interactive)
ssh -o BatchMode=yes -o PubkeyAuthentication=no <server-address>

# 2. the services are not on the network — expect both to fail
nc -z -G 3 <server-address> 8188
nc -z -G 3 <server-address> 11434

# 3. through the tunnel (ssh imagegen running) — expect JSON, then HTTP 200
curl -s http://localhost:21434/api/version
curl -sI http://localhost:8188/ | head -1

# 4. the key runs nothing — expect no output and exit 1
ssh -o SessionType=default imagegen id; echo "exit $?"
```

## When it does not work

| Symptom | Cause |
|---|---|
| `No Ollama at http://localhost:21434` from `enhance.sh` | the tunnel is not up — is `ssh imagegen` running? |
| `ssh imagegen` exits with `Address already in use` | something on the client already has `8188` or `21434` — a local ComfyUI, say |
| `Permission denied (publickey)` | key line missing or in the wrong user's file, or `~/.ssh` / `authorized_keys` too open (`700` / `600`) |
| `channel N: open failed: administratively prohibited` | a tunnel to a port or host spelling not in `permitopen` — `localhost` instead of `127.0.0.1` |
| the key stopped working after editing its line | an option sshd does not know (`permitlisten="none"`, a typo) — the whole line is ignored |
| `ping` gets nothing back, `ssh` works | the macOS firewall's stealth mode; not an error |
| works at home, not outside | VPN not connected, or `HostName` is a `.local` name |
