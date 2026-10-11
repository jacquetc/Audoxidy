# Audoxidy (Teksilo UI): Audio Center: the audio effects tab.
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Card titles (`view_effect`, audio_center.rs:3290-3700)
ac-effects-sub-bass-title = Refuerzo de Sub-Graves
ac-effects-mid-bass-title = Refuerzo de Graves
ac-effects-voice-boost-title = Refuerzo de Voces
ac-effects-noise-gate-title = Reducción de Ruido
ac-effects-stereo-expander-title = Expansor Estéreo
ac-effects-stereo-balance-title = Balance Estéreo
ac-effects-compressor-title = Compresor
ac-effects-limiter-title = Limitador
ac-effects-reverb-title = Reverberación
ac-effects-downmix-lfe-title = Canal de Subwoofer
ac-effects-downmix-center-title = Canal Central
ac-effects-downmix-surround-title = Canales Surround  SL/SR | SBL/SBR

## Parameter labels
ac-effects-level-db = Nivel dB
ac-effects-threshold-db = Umbral dB
ac-effects-width-percent = Ancho %
ac-effects-balance-sides = Left | Right
ac-effects-intensity-percent = Intensidad %
ac-effects-ceiling-db = Techo dB
ac-effects-mix = Mix
ac-effects-room-size = Tamaño

## Expander mode and the downmix heading
ac-effects-mode-natural = Natural
ac-effects-mode-surround = Surround
ac-effects-downmix-heading = Volumen de canales en mezcla menor a 5.1

# Not in the Iced UI: the mode switch has no accessible name there.
ac-effects-expander-mode-name = Modo del expansor estéreo

## Accessible slider names
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-sub-bass-level-name = Refuerzo de Sub-Graves: Nivel dB
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-mid-bass-level-name = Refuerzo de Graves: Nivel dB
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-voice-boost-level-name = Refuerzo de Voces: Nivel dB
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-noise-gate-threshold-name = Reducción de Ruido: Umbral dB
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-stereo-expander-width-name = Expansor Estéreo: Ancho %
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-stereo-balance-balance-name = Balance Estéreo: Left | Right
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-compressor-threshold-name = Compresor: Umbral dB
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-compressor-intensity-name = Compresor: Intensidad %
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-limiter-ceiling-name = Limitador: Techo dB
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-reverb-mix-name = Reverberación: Mix
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-reverb-size-name = Reverberación: Tamaño
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-downmix-lfe-mix-name = Canal de Subwoofer: Mix
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-downmix-center-mix-name = Canal Central: Mix
# Not in the Iced UI: the slider is only labelled by the text beside it, shared by several cards.
ac-effects-downmix-surround-mix-name = Canales Surround: Mix

## Errors
# Not in the Iced UI: an effect that could not be saved shows a toast (quirk fix 8).
ac-effects-save-failed = No se pudo guardar el ajuste de audio
