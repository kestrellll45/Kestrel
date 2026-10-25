# Building Kestrel

## Prerequisites (Windows 10/11 x64)

| Tool | Why | Installed by `setup.ps1` |
|---|---|---|
| Visual Studio 2022 Build Tools, C++ workload | Rust's MSVC linker; compiles `ring` (TLS) | `winget Microsoft.VisualStudio.2022.BuildTools` |
| Rust stable (MSVC toolchain) | launcher | `winget Rustlang.Rustup` |
| JDK 17–21 | runs Gradle 8.8 | `winget EclipseAdoptium.Temurin.17.JDK` |

Gradle itself downloads Java 8 (via the foojay toolchain resolver), Minecraft 1.8.9,
MCP mappings and Forge 11.15.1.2318 on the first build. Expect the first mod build to take
several minutes; later builds are fast.

## Commands

```powershell
scripts\setup.ps1 [-Yes]      # check / install prerequisites
scripts\build.ps1 [-Run]      # dist\Kestrel\Kestrel.exe + dist\Kestrel\client\kestrel-<ver>.jar
scripts\test.ps1              # cargo fmt/clippy/test + gradle test
scripts\package.ps1           # tests + build + dist\Kestrel-<ver>-windows-x64.zip (+ .sha256)
```

(Prefix each with `powershell -ExecutionPolicy Bypass -File` if script execution is disabled.)

Manual equivalents:

```powershell
cd client;   .\gradlew.bat build          # mod -> client\build\libs\kestrel-<ver>.jar
cd launcher; cargo build --release -p kestrel-launcher
```

The launcher looks for the mod in `<launcher exe dir>\client\kestrel-*.jar`
(or `%KESTREL_MOD_JAR%`), and copies it into the game's `mods` folder on Play.

### Developing the mod

`cd client; .\gradlew.bat runClient` starts Minecraft with the mod from source.
DevAuth is included for signing in during development (see its README; disabled unless
configured).

## Standalone release (Kestrel.exe)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release.ps1
```

Output: `release\Kestrel.exe` (plus `Kestrel.exe.sha256.txt`). The script runs the tests,
builds the mod, builds the launcher for `x86_64-pc-windows-msvc` with the mod jar, icon,
fonts and cosmetic previews embedded and the C runtime linked statically, then copies the
exe to a temp folder outside the project and runs `Kestrel.exe --self-check` there against a
throw-away data directory, checks the DLL imports, and prints the final size and location.
`-SkipTests` skips step 1. Full output: `dist\release-log.txt`.

`Kestrel.exe` needs nothing else: no project folder, no .NET, no Java, no VC++ redistributable.
On first Play it downloads Minecraft 1.8.9, Forge and Mojang's Java 8 from the official servers
into `%LOCALAPPDATA%\Kestrel` (these are never bundled, for licensing reasons). Settings,
game files, logs and uploaded cosmetics live there too, so moving or replacing the exe (an
upgrade) keeps everything. A newer exe replaces only the client mod jar in `instance\mods`.

You can run the self-check yourself: `Kestrel.exe --self-check report.txt`.

## First launch

1. Start `Kestrel.exe`. Data goes to `%LOCALAPPDATA%\Kestrel` (override with
   `Kestrel.exe --root D:\Games\Kestrel` or `%KESTREL_HOME%`).
2. **Offline mode** works immediately for singleplayer and offline-mode servers.
3. Press **Play**. The first run downloads a few hundred MB (Minecraft, libraries, assets, Forge,
   Mojang's Java 8 runtime), verifying SHA-1 hashes where Mojang/Forge publish them.
4. The Home page shows "Forge loaded" and "Kestrel loaded" once the game has started.
5. In game, press **Right Shift** for the Kestrel menu.
6. **Cosmetics** (left sidebar of the launcher): equip kill effects, skies and capes, upload
   your own PNGs. Changes apply on the next Play, or within about a second if the game is
   already running.

## Microsoft sign-in (required for online servers such as Hypixel)

Kestrel uses Microsoft's OAuth device-code flow: you sign in on microsoft.com in your
browser and enter a short code. Kestrel never sees your password. Tokens are stored
encrypted with Windows DPAPI (bound to your Windows account) in `accounts.dat`.

Every third-party launcher needs its own app registration in **Microsoft Entra ID**.
Kestrel signs in with the device-code flow against the personal-account authority
`https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode` and opens the
sign-in page Microsoft returns (normally `https://www.microsoft.com/link`).

**0. You need your own Entra directory (tenant).** A personal Microsoft account
(outlook.com, hotmail.com, live.com, gmail-based MSA) that is not a member of any
directory cannot use the Azure portal or Entra admin center: sign-in fails with

> AADSTS50020: User account ... from identity provider 'live.com' does not exist in
> tenant 'Microsoft Services' and cannot access the application
> '74658136-14ec-4630-ad9b-26e160ff0fc6'(ADIbizaUX) in that tenant.

That error comes from the **Azure portal** (`ADIbizaUX` is the portal's own app),
not from Kestrel. Fix it once by creating a directory: in an InPrivate window open
<https://azure.microsoft.com/free>, sign in with your personal account and complete
the free sign-up. It creates a *Default Directory* in which you are Global
Administrator. (Alternatively, ask an admin of an existing tenant to invite you.)

1. Open <https://entra.microsoft.com>. If you belong to several directories, switch to
   yours (gear icon → **Directories + subscriptions**). Go to **Entra ID** →
   **App registrations** → **New registration**.
2. Name: anything (e.g. "Kestrel Launcher"). Supported account types:
   **Personal Microsoft accounts only** (this matches Kestrel's `/consumers`
   authority). No redirect URI.
3. Open the new app → **Authentication** → **Advanced settings** → set
   **Allow public client flows** to **Yes** → **Save**. No client secret or certificate.
4. On the app's **Overview** page copy **Application (client) ID** (a GUID) into
   Kestrel → Settings → Advanced. Do *not* use the Object ID, the Directory (tenant)
   ID, or any ID shown in a Microsoft error page; Kestrel rejects non-GUIDs and the
   Azure portal's ID with an explanation.
5. **Mojang approval:** new app IDs must be approved before Minecraft's services accept
   them. Submit the app ID at <https://aka.ms/mce-reviewappid>. Until approved, sign-in
   fails at the last step with "Minecraft services rejected this application ID (HTTP 403)".

Then Account → **Sign in with Microsoft** shows a code and a button that opens
Microsoft's sign-in page; sign in there with the Microsoft account that owns Minecraft.

## What has and has not been verified

See the "Verification status" section of ARCHITECTURE.md.
