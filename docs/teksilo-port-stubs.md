# Teksilo UI: features that are stubs in the Iced UI

The Teksilo UI ports the Iced UI with strict parity. Where the Iced UI has an entry that does
nothing, or a path that is only half wired, the Teksilo UI keeps the same gap. The code is
marked so the gap stays visible and searchable:

- `// TODO(iced-stub): <what the Iced UI does instead>` at the place where the feature would go.
- When a body is needed, it is `unimplemented!("<feature>: stub in the Iced UI")`.
- The UI entry that leads to it is **disabled**, so the `unimplemented!()` cannot be reached
  from the interface.

These markers are the only `TODO` and `unimplemented!` allowed in `src/logic`, `src/frontend` and
`src/teksilo_ui`. A check in `tools/` fails the build gates if a marker is missing from this
table, or if a row points at a site that has no marker.

Deliberate behaviour changes are listed in [`teksilo-port-quirk-fixes.md`](teksilo-port-quirk-fixes.md).

The **Teksilo site** column is filled in by the wave that ports each stub.

## Disabled entries

| Feature | Iced location | Iced behaviour | Teksilo site |
| --- | --- | --- | --- |
| Playlist menu "Información" | `src/gui/playlist.rs:1499` | `Message::NoOp` | |
| Playlist menu "Editor de Etiquetas" | `src/gui/playlist.rs:1524` | `Message::NoOp` | |
| Playlist menu "Convertidor de audio" | `src/gui/playlist.rs:1529` | `Message::NoOp` | |
| Playlist bottom bar Lyrics button | `src/gui/app.rs:3027` | `ToggleLyrics` returns `Task::none()` | |
| Library source tabs Spotify, YouTube, Qobuz, Deezer, Tidal | `src/gui/library.rs:2479-2483`, `src/gui/app.rs:3068` | only changes which tab is highlighted | `src/teksilo_ui/shell/title_bar.rs` (`Source::is_available`) |
| Player menu "Abrir Archivo" | `src/gui/player.rs:546` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Biblioteca" | `src/gui/player.rs:548` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Lista de Reproducción" | `src/gui/player.rs:549` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Lyrics" (Iced label "Lirycs") | `src/gui/player.rs:550` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Apagado Automático" | `src/gui/player.rs:567` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Personalización" | `src/gui/player.rs:568` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Preferencias" | `src/gui/player.rs:569` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Complementos" | `src/gui/player.rs:570` | closes the menu |  `src/teksilo_ui/player/menu.rs` |
| Player menu "Acerca" | `src/gui/player.rs:571` | closes the menu |  `src/teksilo_ui/player/menu.rs` |

## Half-wired paths

These entries stay enabled and behave as in Iced. The marker sits where Iced stops.

| Feature | Iced location | Iced behaviour | Teksilo site |
| --- | --- | --- | --- |
| Folder node in the filter tree | `src/gui/app.rs:4750` | selects the node but applies no library filter (`SelectFolder` is never emitted) | `src/teksilo_ui/filters/filters_vm.rs` (`select_node`) |
| Meter "Tiempo de hold del pico" stepper | `src/gui/app.rs:5330` | value is saved; the meter keeps the 1.5 s constant (`src/gui/app.rs:496`) | `src/frontend/commands/meter.rs` (`set_hold_time_ms`) |
| Meter "Ventana RMS" stepper | `src/gui/app.rs:5349` | value is saved; nothing reads it | `src/frontend/commands/meter.rs` (`set_rms_window_ms`) |
