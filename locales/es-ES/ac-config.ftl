# Audoxidy (Teksilo UI): Audio Center: the audio configuration tab.
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Settings (left column)
ac-config-host-label = Núcleo de Audio:
ac-config-device-label = Dispositivo de Salida:
ac-config-sample-rate-label = Frecuencia de Muestreo:
ac-config-bit-depth-label = Profundidad de Bits:
ac-config-channels-label = Canales de Salida:
ac-config-buffer-label = Quantum (Buffer):
ac-config-system-title = Pipewire | PulseAudio
ac-config-system-lines = Frecuencia | Quantum
# Not in the Iced UI: accessible name of the audio core pick list, which has only a visible label beside it.
ac-config-host = Núcleo de Audio
# Not in the Iced UI: accessible name of the output device pick list.
ac-config-device = Dispositivo de Salida
# Not in the Iced UI: accessible name of the sample rate pick list.
ac-config-sample-rate = Frecuencia de Muestreo
# Not in the Iced UI: accessible name of the bit depth pick list.
ac-config-bit-depth = Profundidad de Bits
# Not in the Iced UI: accessible name of the output channels pick list.
ac-config-channels = Canales de Salida
# Not in the Iced UI: accessible name of the buffer pick list.
ac-config-buffer = Quantum (Buffer)
# Not in the Iced UI: accessible name of the sound server rate pick list, whose label is shared with the quantum one.
ac-config-system-rate = Frecuencia del servidor de sonido
# Not in the Iced UI: accessible name of the sound server quantum pick list.
ac-config-system-quantum = Quantum del servidor de sonido
ac-config-automatic = Automático
ac-config-system-default = Default
ac-config-channels-other = { $count } canales

## Information (right column)
ac-config-info-title = Información de Audio
ac-config-song = Canción:
ac-config-format = Formato:
ac-config-frequency = Frecuencia:
ac-config-depth = Profundidad:
ac-config-channels-short = Canales:
ac-config-input = Entrada
ac-config-output = Salida
ac-config-gain = Ganancia:
ac-config-gain-lines = Album | Canción
ac-config-quantum = Quantum de Audoxidy:
ac-config-server = Pipewire | PulseAudio:
ac-config-idle = Sin reproducción

## Buttons
ac-config-restart = Reiniciar Servicio de Audio
ac-config-reset = Predeterminado
ac-config-apply = Aplicar

## Errors
# Not in the Iced UI: the Iced UI only logs the failure (quirk fix 8). The body is its log text.
ac-config-apply-failed = No se pudo aplicar la configuración de audio
ac-config-apply-failed-body = El dispositivo puede no soportar la combinación de tasa de muestreo y canales elegida ({ $error }). La reproducción queda detenida hasta aplicar una configuración válida.
