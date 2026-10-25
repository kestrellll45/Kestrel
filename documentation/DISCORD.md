# Discord Rich Presence

Kestrel can show its activity in your Discord profile:

| Situation | Discord shows |
|---|---|
| Launcher open, game not running | In the launcher |
| Downloading / starting the game | Starting Minecraft 1.8.9 |
| Game running, before the mod reports | Playing Minecraft 1.8.9 |
| Main menu | In the main menu |
| Singleplayer world (integrated server, incl. LAN host) | Playing singleplayer |
| Server | Playing multiplayer |

Second line "Minecraft 1.8.9", elapsed time since the launcher opened or the game started,
large image = art asset `kestrel`. Server addresses, world names and account details are
never sent. The only peer is the Discord app on your PC (local IPC); nothing goes over the
network from Kestrel for this.

## One-time setup (needs your Discord account)

Discord shows the application's **name** and **images** from its Developer Portal, so the
app has to exist there. Kestrel does not ship an application ID.

1. Open <https://discord.com/developers/applications> → **New Application** → name it
   `Kestrel`.
2. **Rich Presence → Art Assets**: upload the Kestrel logo
   (`launcher/crates/kestrel-launcher/assets/kestrel.png`, at least 512×512 recommended)
   with the key `kestrel`.
3. **General Information**: copy the **Application ID**.
4. Kestrel → **Settings → Advanced**: tick **Discord Rich Presence** (on by default) and
   paste the ID. The status line says "Connected to Discord." once it works.

To bake the ID into release builds instead, it can be set as the default in
`LauncherConfig` later; it is a public identifier, not a secret.

## How it works

* `kestrel-core/src/discord.rs`: IPC client (`\\?\pipe\discord-ipc-0..9` on Windows,
  `discord-ipc-N` sockets on Linux/macOS), handshake `{"v":1,"client_id":…}` → READY,
  then `SET_ACTIVITY`. One background thread (`PresenceService`) reconnects with backoff
  (5 s → 60 s) while Discord is closed, coalesces updates (≥ 2 s apart), and clears the
  activity when the launcher exits or the setting is turned off. It never starts processes
  and never blocks the UI; shutdown waits at most 0.5 s.
* Game state: the mod's `ActivityReporter` logs `[Kestrel] activity <token>: <state>` on
  change. `<token>` is random per launch (`-Dkestrel.activityToken`), so a chat message
  that quotes the marker cannot spoof the state. The launcher parses it in
  `launch::parse_activity`.

## Limitations

* Presence comes from the launcher process. With **Close the launcher once the game has
  loaded** enabled, Discord clears the activity when the launcher closes.
* Not verified against a real Discord client yet: the protocol is tested end to end against
  a fake Discord IPC server (`discord::tests::fake_discord`), and the Windows named-pipe path
  was not compiled in the development environment.
