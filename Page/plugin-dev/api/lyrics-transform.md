# Lyrics Transform API

`LyricsTransformApiV2` registers a callback that changes parsed lyric text before display. Declare `CAP_LYRICS` and query `IFACE_LYRICS_TRANSFORM`. The SDK `host.lyrics()?.register(|line| ...)` manages callback storage for a text-to-text closure.

## A typical use

Register one transformer during `create`, retain its `Resource`, and drop it during `shutdown`. For example, a plugin can replace a known spelling in displayed lines:

```rust
let transform = host.lyrics()?.register(|line| line.replace("colour", "Colour"))?;
// Keep `transform` in the plugin instance.
```

This example preserves the number of Unicode characters, which matters for word-synchronised lyrics. The SDK closure receives only text, so use count-preserving changes for every line. The raw callback receives `LYRICS_TEXT_FLAG_WORD_SYNCED` if you need to treat synchronised lines differently.

## Methods

Both methods take `context, token` first and return `PluginStatus`.

| Method | Remaining parameters | Result |
|---|---|---|
| `register` | `*const LyricsTransformerDataV2`, `*mut ResourceId` | Registers a transformer and writes its ID. |
| `release` | `ResourceId` | Removes it when no callback is in flight. |

## Transformation callback

`LyricsTransformerDataV2` requires `on_transform`; `flags` is zero, `LYRICS_TRANSFORMER_FLAG_SECONDARY`, or `LYRICS_TRANSFORMER_FLAG_FONT_FAMILY`. These output modes cannot be combined. The callback receives a `LyricsTextV2` containing the line timestamp, flags, and borrowed UTF-8 text. It is called twice for each line: first with null output and zero capacity to ask for the required byte length, then with a writable buffer. Write `out_len` in both passes and return `PluginStatus::Ok` on success. Keep the output stable between passes.

When `LYRICS_TEXT_FLAG_WORD_SYNCED` is set, preserve the same Unicode character count to keep word timing boundaries. The input text is valid only during the callback. Keep `callback_data` valid until release succeeds; releasing during an active callback returns `LimitExceeded`. The current quota is four transformers per plugin.

## Secondary text

Register with `LYRICS_TRANSFORMER_FLAG_SECONDARY`, or call `host.lyrics()?.register_secondary(|line| ...)`, to supply text shown below each line, such as a translation, instead of replacing the line. The host calls secondary transformers only while Show Secondary Lyrics is on, after the original lines are displayed, one line at a time in song order. It waits up to 180 seconds for each call, so a transformer can process the whole song on its first line and answer later lines from a cache, and it shows the lines answered so far about once a second. Return an empty string to leave a line without secondary text; word timing is unaffected. Once the track changes, the host stops asking and requests already queued for that song are skipped without calling the plugin.

All of a plugin's callbacks run one at a time on its worker, so while a secondary call runs, the plugin's widget ticks, settings changes, and media events wait for it. The host skips the plugin's primary transformer while that worker is busy, and a settings change still times out after 2 seconds. A call that exceeds 180 seconds makes the host skip the plugin until that call returns. Disabling, updating, or uninstalling the plugin waits at most 2 seconds for a running call and otherwise fails, keeping the plugin loaded, so keep each call as short as the work allows.

`LyricsTextV2` also carries `line_index`, `line_count`, and `song`, every line joined by a line feed (`\n`), for every kind of transformer. Check `struct_size` before reading these fields on an older host. When a secondary transformer supplies any text for a song, provider translations are not shown for that song; they are shown as before when no secondary transformer is registered.

## Per-line font selection

Use `host.lyrics()?.register_font_family(|line| ...)` to return `Some(family_name)` for an original lyric line or `None` to preserve its current font. The raw API uses `LYRICS_TRANSFORMER_FLAG_FONT_FAMILY` and returns the family name through the same host-owned output buffer; an empty output leaves the selection unchanged. Font callbacks run after primary text transformations and do not modify UTF-8 text, translations, or word timings. Family names must contain no NUL bytes and fit within `LYRICS_FONT_FAMILY_MAX_BYTES`. Hosts predating this flag reject registration.

Native text rendering shapes Bengali runs before measuring and drawing them, including word-highlight boundaries. Font selection is included in the text cache keys so different line fonts keep separate measurements and glyphs.

See the [lyrics example](https://github.com/WinIslandProject/WinIsland/blob/master/crates/winisland-plugin-api/examples/lyrics_transform.rs) and [callback type](https://github.com/WinIslandProject/WinIsland/blob/master/crates/winisland-plugin-api/src/types/v2/lyrics.rs).

[All plugin APIs](/plugin-dev/api)
