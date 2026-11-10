# Privacy Policy

English | [简体中文](./PRIVACY-zh.md)

This policy explains how WinIsland handles information. It applies to every WinIsland build published by the WinIslandProject, including GitHub releases, nightly builds, and packaged builds.

## Summary

- WinIsland is a desktop application with no WinIsland accounts, advertising, analytics, telemetry, or crash reporting service.
- Optional network features contact the services described below. WinIsland does not upload your information to a project telemetry service.
- Update checks, online lyrics, the plugin marketplace, and the optional LingoSoul assistant use the network. Configured AI services process assistant messages according to their own settings and privacy policies.

## Information processed on your device

WinIsland reads the following information to display it on the island. It is processed in memory on your computer and is not sent to the WinIsland project.

| Information | Why it is used | How to control it |
| --- | --- | --- |
| Now-playing media (title, artist, album, artwork, playback state and position, and the app that is playing) | Show music controls, artwork, progress, and lyrics | **SMTC Control**, and the list of allowed media apps |
| Audio output of the media app and the system | Animate the audio visualizer while a media app is active. Audio is analyzed in memory and is never recorded, saved, or transmitted | **SMTC Control** |
| Windows notifications (app name, icon, title, and text) | Show incoming notifications on the island. Off by default | **Notification Display** |
| Volume keys (Volume Up, Volume Down, Mute) | Replace the system volume flyout. Only these three keys are acted on; every other keystroke is passed through untouched and never read or recorded | **System Volume and Brightness Controls** |
| Text you copy to the clipboard | Detect a copied link and ask whether to open it. Clipboard contents are not stored or transmitted | **Ask to Open Copied Links** |
| Whether a microphone or camera is in use | Show a privacy indicator. This indicator does not capture microphone audio or camera video; LingoSoul voice input is described below | |
| CPU, memory, GPU, network, and disk usage | Resource usage widgets | Remove the widgets |
| Foreground window and full-screen state | Hide the island while full-screen apps are running | Auto-hide settings |

## Information stored on your device

| Location | Contents |
| --- | --- |
| `%USERPROFILE%\.winisland\config.toml` | Your settings, including any custom font path, local lyrics folder, and media app lists |
| `%USERPROFILE%\.winisland\logs\` | Diagnostic logs. They may contain track titles and artists, lyrics lookups, and error messages. Logs are never uploaded automatically. If you attach them to an issue, review them first |
| `%USERPROFILE%\.winisland\updates\` | Installers downloaded by the updater |
| `%APPDATA%\WinIsland\plugins\` | Installed plugins and their files |
| `%USERPROFILE%\.winisland-lingosoul\config.toml` | LingoSoul settings, including the custom AI endpoint, API key, local Agent permissions, working directory, and additional read-only directories |
| `%USERPROFILE%\.winisland-lingosoul\history.jsonl` | LingoSoul conversation history. Retention and clearing are available in the LingoSoul settings |
| `%USERPROFILE%\.winisland-lingosoul\prompts\` | Editable assistant instructions and response styles |
| `%LOCALAPPDATA%\WinIsland\lingosoul\models\` | Speech models and resumable downloads |

These are the default locations. `LINGOSOUL_CONFIG` can move the assistant configuration, history and default prompts. `LINGOSOUL_MODELS` can select a model directory; existing models under `%LOCALAPPDATA%\LingoSoul\models\` can also be reused. Stop WinIsland before removing its files. AI providers manage their own data separately; deleting local files does not delete data held by those services. Do not attach assistant configuration files containing API keys to issues.

## Network connections

The features below make network requests, which reveal your IP address to the service being contacted. Configured AI services also receive the credentials and conversation data needed for assistant requests.

### Update checks

When **Check for Updates** is on (default), WinIsland periodically asks GitHub (`api.github.com` and `github.com`) whether a newer release is available. An installer is downloaded only after you confirm the update, and its SHA-256 checksum is verified before it runs. GitHub's privacy statement applies to these requests.

### Online lyrics

When **Show Lyrics** is on and **Lyrics Mode** is set to **Online** (both default), WinIsland sends the title and artist of the current track, and for some providers its duration, to look up lyrics. The request goes to the selected provider, and to the others as fallbacks if it finds nothing:

- AMLL (`api.amll.dev`)
- LRCLIB (`lrclib.net`)
- NetEase Cloud Music (`music.163.com`)
- QQ Music (`y.qq.com`)
- Kugou (`kugou.com`)

These services are operated by third parties, and their own privacy policies apply. To stop these requests, set **Lyrics Mode** to **LRC Folder** or turn off **Show Lyrics**.

### Plugin marketplace

When you open the plugin marketplace or install a plugin from it, WinIsland downloads the catalog, icons, and plugin packages from GitHub (`WinIslandProject/PluginMarketplace` and the repositories it lists).

### LingoSoul assistant

LingoSoul is an optional component running inside WinIsland. Builds without the `lingosoul` feature do not include its runtime or settings page. In builds that include it, **Enable LingoSoul** controls the assistant. Voice input uses the microphone and local speech models; audio is processed in memory, and recognized text is sent to the configured AI endpoint. Speech models download from GitHub only when you use the model-download control.

The custom AI endpoint receives your messages, conversation context and tool results. A local model endpoint is also supported. Built-in tools and WinIsland plugin tools are available to the assistant. Weather requests use Open-Meteo and, when a city is omitted, IPWhois (`ipwho.is`) for approximate IP-based location.

The optional local Agent provides read, write, update and execute tools. File reads are limited to the working directory and additional read-only directories; writes and updates are limited to the working directory. Read content and command output can be sent to your configured AI endpoint as tool results. Execute permission launches commands with your Windows account permissions: the working directory is not an operating-system sandbox. External Agent clients and MCP servers are not used.

Turn off **Enable LingoSoul** to stop the assistant. Local Agent permissions can be controlled separately. History retention and clear-history controls apply to local history, not to records kept by AI services. Closing the conversation panel keeps the conversation available through the multitask entry; it does not cancel ongoing work.

### Links you open

Links you choose to open, including copied links you confirm through **Ask to Open Copied Links**, open in your default browser.

## Plugins

Plugins are extensions that you install yourself. They run with the same permissions as WinIsland and may process data or connect to the internet on their own, for example to fetch weather. The WinIsland project does not control third-party plugins. Review a plugin and its author's privacy practices before installing it, and remove the plugin from the **Plugins** page to stop it.

## Your choices

You can turn off the features above in Settings and remove their local data as described above. For messages processed by a remote AI service, use that service's controls to manage its records. WinIsland cannot delete records held by those services.

## Children's privacy

WinIsland does not collect personal information from anyone, including children.

## Changes to this policy

Changes to WinIsland's data handling are reflected in this file. The full revision history is available in this repository's commit log.

## Contact

Questions about this policy can be asked in [GitHub Issues](https://github.com/WinIslandProject/WinIsland/issues). Please report security or privacy vulnerabilities privately as described in the [Security Policy](SECURITY.md).
