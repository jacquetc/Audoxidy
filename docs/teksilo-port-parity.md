# Teksilo UI: parity with the Iced UI

This file maps every behaviour of the Iced UI (`src/gui/`) to where the Teksilo UI
(`src/teksilo_ui/`) provides it, and to the test or probe that checks it. It is the checklist the
final review works through: a row with no Teksilo location and no check means parity is not
done.

Rows come from three sources:
- every variant of the Iced `Message` enum (`src/gui/app.rs`) and of `AudioCenterMessage`
  (`src/gui/audio_center.rs`);
- every interactive element of the Iced views;
- every persistence key.

Status markers:
- **Variants not ported:** dead `Message` variants (handled but never emitted) are listed with
  status `dead`.
- **Stubs:** see [`teksilo-port-stubs.md`](teksilo-port-stubs.md). They are listed with status
  `stub`.
- **Behaviour changes:** see [`teksilo-port-quirk-fixes.md`](teksilo-port-quirk-fixes.md). They
  are listed with status `fixed #N`.

Every table below uses the same columns: `Iced | Behaviour | Teksilo | Check | Status`.

## Playback and transport

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Playlist

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Library

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Filters

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Audio Center

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## VU meter

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Window, dialogs and menus

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Keyboard

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |

## Persistence keys

| Iced | Behaviour | Teksilo | Check | Status |
| --- | --- | --- | --- | --- |
