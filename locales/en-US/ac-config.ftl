# Audoxidy (Teksilo UI): Audio Center: the audio configuration tab.
# Source locale: `tr!` validates keys against these files.

## Settings (left column)
ac-config-host-label = Audio Core:
ac-config-device-label = Output Device:
ac-config-sample-rate-label = Sample Rate:
ac-config-bit-depth-label = Bit Depth:
ac-config-channels-label = Output Channels:
ac-config-buffer-label = Quantum (Buffer):
ac-config-system-title = Pipewire | PulseAudio
ac-config-system-lines = Rate | Quantum
ac-config-host = Audio core
ac-config-device = Output device
ac-config-sample-rate = Sample rate
ac-config-bit-depth = Bit depth
ac-config-channels = Output channels
ac-config-buffer = Buffer quantum
ac-config-system-rate = Sound server rate
ac-config-system-quantum = Sound server quantum
ac-config-automatic = Automatic
ac-config-system-default = Default
ac-config-channels-other = { $count } channels

## Information (right column)
ac-config-info-title = Audio Information
ac-config-song = Song:
ac-config-format = Format:
ac-config-frequency = Rate:
ac-config-depth = Depth:
ac-config-channels-short = Channels:
ac-config-input = Input
ac-config-output = Output
ac-config-gain = Gain:
ac-config-gain-lines = Album | Song
ac-config-quantum = Audoxidy quantum:
ac-config-server = Pipewire | PulseAudio:
ac-config-idle = Not playing

## Buttons
ac-config-restart = Restart Audio Service
ac-config-reset = Default
ac-config-apply = Apply

## Errors
ac-config-apply-failed = The audio settings could not be applied
ac-config-apply-failed-body = The device may not support the selected sample rate and channel combination ({ $error }). Playback stays stopped until a valid configuration is applied.
