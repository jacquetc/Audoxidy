#!/usr/bin/env python3
"""Builds music libraries for live checks of the Audoxidy UIs, from a few audio samples.

The samples are only read: every file is a new encoding (or a stream copy) written under the
output directory, with its own tags and, for most albums, its own embedded cover.

    tools/make_test_library.py --source ~/Musique --out ~/Devel/audoxidy-test-music

Two libraries are written:

- ``diverse/``: some forty albums that cover what the library, the filters and the playlist must
  handle: every format the scanner reads that ffmpeg can write (MP3, FLAC 16/44.1, FLAC 24/96,
  FLAC 5.1, Ogg Vorbis, Opus, AAC in M4A, WAV, AIFF, WavPack), compilations, two albums with the
  same title by different artists, multi-disc albums with a ``CD 02`` folder, names with accents,
  CJK characters and a ``|``, missing track numbers, years and genres, full dates as years,
  ``n/total`` track numbers, long titles, ReplayGain tags, and albums without a cover.
- ``large/``: ``--large`` albums (2280 by default, the size of the GPU-memory check) of two MP3
  songs each, every album with a distinct cover, spread over 400 artists, 26 genres and 66 years.

Needs ``ffmpeg`` on PATH and Pillow. Nothing is written outside ``--out``; an existing file is
kept unless ``--force`` is given, so a rerun only adds what is missing.
"""

import argparse
import colorsys
import concurrent.futures
import hashlib
import io
import json
import os
import random
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# --- covers -------------------------------------------------------------------------------------


def cover_jpeg(seed: str, title: str, artist: str, size: int = 600) -> bytes:
    """A deterministic cover: a two-colour gradient from `seed`, the title and the artist."""
    digest = hashlib.sha256(seed.encode()).digest()
    hue_a, hue_b = digest[0] / 255.0, digest[1] / 255.0
    top = tuple(int(c * 255) for c in colorsys.hsv_to_rgb(hue_a, 0.55, 0.85))
    bottom = tuple(int(c * 255) for c in colorsys.hsv_to_rgb(hue_b, 0.75, 0.35))
    image = Image.new("RGB", (size, size))
    draw = ImageDraw.Draw(image)
    for y in range(size):
        t = y / (size - 1)
        draw.line([(0, y), (size, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)))
    try:
        big = ImageFont.truetype("DejaVuSans-Bold.ttf", size // 12)
        small = ImageFont.truetype("DejaVuSans.ttf", size // 18)
    except OSError:
        big = small = ImageFont.load_default()
    draw.text((size // 16, size - size // 4), title[:28], fill="white", font=big)
    draw.text((size // 16, size - size // 8), artist[:36], fill="white", font=small)
    shape = digest[2] % 3
    box = [size // 4, size // 6, 3 * size // 4, size // 6 + size // 2]
    if shape == 0:
        draw.ellipse(box, outline="white", width=size // 60)
    elif shape == 1:
        draw.rectangle(box, outline="white", width=size // 60)
    else:
        draw.polygon([(size // 2, box[1]), (box[2], box[3]), (box[0], box[3])], outline="white")
    out = io.BytesIO()
    image.save(out, "JPEG", quality=85)
    return out.getvalue()


# --- formats ------------------------------------------------------------------------------------

# extension, ffmpeg audio options, whether ffmpeg can embed a cover in this container
FORMATS = {
    "mp3": ("mp3", ["-c:a", "libmp3lame", "-b:a", "192k", "-id3v2_version", "3"], True),
    "mp3-copy": ("mp3", ["-c:a", "copy", "-id3v2_version", "3"], True),
    "flac16": ("flac", ["-c:a", "flac", "-sample_fmt", "s16", "-ar", "44100"], True),
    "flac24-96": ("flac", ["-c:a", "flac", "-sample_fmt", "s32", "-bits_per_raw_sample", "24", "-ar", "96000"], True),
    "flac-5.1": (
        "flac",
        [
            "-af", "pan=5.1|FL=c0|FR=c1|FC=0.5*c0+0.5*c1|LFE=0.1*c0|BL=0.7*c0|BR=0.7*c1",
            "-c:a", "flac", "-sample_fmt", "s16", "-ar", "48000",
        ],
        True,
    ),
    "ogg": ("ogg", ["-c:a", "libvorbis", "-q:a", "5"], False),
    "opus": ("opus", ["-c:a", "libopus", "-b:a", "128k"], False),
    "m4a": ("m4a", ["-c:a", "aac", "-b:a", "192k"], True),
    "wav": ("wav", ["-c:a", "pcm_s16le"], False),
    "aiff": ("aiff", ["-c:a", "pcm_s16be", "-write_id3v2", "1"], False),
    "wv": ("wv", ["-c:a", "wavpack"], False),
}


def encode(sample: Path, out: Path, fmt: str, tags: dict, cover: bytes | None, force: bool) -> str:
    """Writes `out` from `sample` in `fmt` with `tags` and, where the container allows, `cover`."""
    if out.exists() and not force:
        return "kept"
    ext, audio_opts, can_cover = FORMATS[fmt]
    out.parent.mkdir(parents=True, exist_ok=True)
    cmd = ["ffmpeg", "-nostdin", "-loglevel", "error", "-y", "-i", str(sample)]
    cover_path = None
    if cover is not None and can_cover:
        cover_path = out.with_suffix(".cover.jpg")
        cover_path.write_bytes(cover)
        cmd += ["-i", str(cover_path), "-map", "0:a", "-map", "1:0", "-c:v", "copy",
                "-disposition:v", "attached_pic", "-metadata:s:v", "comment=Cover (front)"]
    else:
        cmd += ["-map", "0:a"]
    cmd += audio_opts
    cmd += ["-map_metadata", "-1"]
    for key, value in tags.items():
        if value is not None:
            cmd += ["-metadata", f"{key}={value}"]
    tmp = out.with_name(out.stem + ".part." + ext)
    cmd.append(str(tmp))
    try:
        subprocess.run(cmd, check=True)
        tmp.replace(out)
    finally:
        if cover_path is not None:
            cover_path.unlink(missing_ok=True)
        tmp.unlink(missing_ok=True)
    return "written"


def safe(name: str) -> str:
    """A file-name-safe form of `name` (the tags keep the original)."""
    return "".join("_" if c in '/\\:*?"<>|' else c for c in name).strip() or "_"


# --- the diverse library ------------------------------------------------------------------------

LONG = "A Very Long Song Title That Scrolls Across The Player Because It Does Not Fit"


def diverse_albums() -> list[dict]:
    """The albums of `diverse/`, each a dict of album-level fields and a list of tracks."""
    def album(artist, title, year, genre, fmt, n, **extra):
        return {"artist": artist, "album": title, "year": year, "genre": genre, "format": fmt,
                "tracks": [{"title": f"{title} {i + 1}"} for i in range(n)], **extra}

    albums = [
        album("Anna Keller", "Piano Sonatas", "1998", "Classical", "flac24-96", 6),
        album("Anna Keller", "Late Works", "2004", "Classical", "flac16", 5),
        album("Iron Cathedral", "Forge", "2011", "Metal", "mp3", 8, replaygain=True),
        album("Iron Cathedral", "Greatest Hits", "2015", "Metal", "mp3", 4),
        album("Neon Parade", "Greatest Hits", "2016", "Pop", "m4a", 4),
        album("Los Caminantes", "Tierra Adentro", "1987", "Folk", "flac16", 7),
        album("Marisol Vega", "Cartas de Amor", "1979", "Bolero", "ogg", 6),
        album("Björk Ñandú", "Canción Eléctrica", "2020", "Electrónica", "opus", 5),
        album("坂本龍一", "音楽図鑑", "1984", "Ambient", "flac16", 4),
        album("AC|DC Tribute", "Pipe | Dreams", "2002", "Rock", "mp3", 4),
        album("Quartet Bleu", "Late Set", "1961", "Jazz", "wav", 3),
        album("Pulse Theory", "Floorwork", "2019", "Electronic", "flac-5.1", 4, replaygain=True),
        album("Hollow Orchard", "Small Rooms", None, None, "mp3", 5),
        album("Lunar Bloom", "Half Asleep", "2008-05-01", "Dream Pop", "aiff", 3),
        album("Kraftwerk Echo", "Machine Dreams", "1983", "Electronic", "wv", 4),
        album("Otis Clay Jr.", "Sunday Soul", "1972", "Soul", "mp3-copy", 6, no_track_numbers=True),
        album("Delta Slim", "Crossroads Dust", "1958", "Blues", "mp3", 5, no_cover=True),
        album("Mirror Season", "Slow Weather", "2013", "Indie", "ogg", 4),
        album("Rapid Fire", "Breakbeat Theory", "1996", "Drum & Bass", "m4a", 6, replaygain=True),
        album("Nordic Carol Choir", "Winter Evening", "2010", "Choral", "flac16", 5, no_cover=True),
        album("The Long Names Society", LONG, "2021", "Indie", "mp3", 3, long_titles=True),
    ]
    # A double album: CD 01 and CD 02 folders, disc numbers.
    double = album("Ennio Marchetti", "Cinema Paradiso Suites", "1990", "Soundtrack", "flac16", 10)
    for i, track in enumerate(double["tracks"]):
        track["disc"] = "1/2" if i < 5 else "2/2"
        track["track"] = f"{i % 5 + 1}/5"
        track["folder"] = "CD 01" if i < 5 else "CD 02"
    albums.append(double)
    # Compilations: one album artist, many track artists.
    for n, (title, year, fmt) in enumerate([("Summer Hits 1999", "1999", "mp3"),
                                             ("Jazz Moods", "2005", "flac16")]):
        comp = album("Varios Artistas", title, year, "Compilation", fmt, 8)
        for i, track in enumerate(comp["tracks"]):
            track["artist"] = ["Neon Parade", "Marisol Vega", "Rapid Fire", "Quartet Bleu",
                               "Delta Slim", "Lunar Bloom", "Iron Cathedral", "Anna Keller"][(i + n) % 8]
        comp["compilation"] = True
        albums.append(comp)
    # Many small albums by one artist (a long artist block, a long grid row).
    for year in range(1969, 1990, 3):
        albums.append(album("David Bowman", f"Studio Album {year}", str(year), "Rock",
                            ["flac16", "mp3", "m4a", "flac24-96"][year % 4], 3))
    return albums


def diverse_jobs(source: Path, out: Path, samples: list[Path]) -> list[tuple]:
    jobs = []
    for a_idx, album in enumerate(diverse_albums()):
        artist = album["artist"]
        cover = None if album.get("no_cover") else cover_jpeg(artist + album["album"], album["album"], artist)
        folder = out / "diverse" / safe(artist) / safe(f"{album['year'] or 'Unknown'} - {album['album']}")
        total = len(album["tracks"])
        for t_idx, track in enumerate(album["tracks"]):
            sample = samples[(a_idx + t_idx) % len(samples)]
            title = track["title"]
            if album.get("long_titles"):
                title = f"{LONG} ({t_idx + 1})"
            tags = {
                "title": title,
                "artist": track.get("artist", artist),
                "album": album["album"],
                # Every fourth album has no album-artist tag: the grouping falls back to the artist.
                "album_artist": None if a_idx % 4 == 3 and not album.get("compilation") else artist,
                "genre": album["genre"],
                "date": album["year"],
                "track": None if album.get("no_track_numbers") else track.get("track", f"{t_idx + 1}/{total}"),
                "disc": track.get("disc"),
                "compilation": "1" if album.get("compilation") else None,
                "comment": "Audoxidy test library",
            }
            if album.get("replaygain"):
                tags["REPLAYGAIN_TRACK_GAIN"] = f"{-3.0 - t_idx * 0.7:.2f} dB"
                tags["REPLAYGAIN_TRACK_PEAK"] = "0.988"
                tags["REPLAYGAIN_ALBUM_GAIN"] = "-5.40 dB"
                tags["REPLAYGAIN_ALBUM_PEAK"] = "0.999"
            ext = FORMATS[album["format"]][0]
            name = f"{t_idx + 1:02d} - {safe(title)[:60]}.{ext}"
            path = folder / track["folder"] / name if "folder" in track else folder / name
            jobs.append((sample, path, album["format"], tags, cover))
    return jobs


# --- the large library --------------------------------------------------------------------------

GENRES = ["Rock", "Pop", "Jazz", "Blues", "Classical", "Electronic", "Folk", "Soul", "Funk",
          "Reggae", "Metal", "Punk", "Hip-Hop", "Ambient", "Country", "Latin", "Bolero", "Tango",
          "Disco", "House", "Techno", "Gospel", "Indie", "Soundtrack", "World", "Chanson"]
WORDS = ["Blue", "Night", "River", "Glass", "Echo", "Silver", "Northern", "Lost", "Velvet", "Iron",
         "Paper", "Golden", "Quiet", "Electric", "Hollow", "Crimson", "Morning", "Wild", "Distant",
         "Broken", "Summer", "Winter", "Neon", "Stone", "Ocean", "Desert", "City", "Garden"]
NOUNS = ["Lights", "Roads", "Hearts", "Machines", "Rooms", "Waves", "Stories", "Signals", "Dreams",
         "Fires", "Mirrors", "Songs", "Rivers", "Engines", "Shadows", "Letters", "Birds", "Bells"]


def large_jobs(out: Path, samples: list[Path], count: int) -> list[tuple]:
    rng = random.Random(2280)
    artists = sorted({f"{rng.choice(WORDS)} {rng.choice(NOUNS)}" for _ in range(2000)})[:400]
    shortest = min(samples, key=lambda p: p.stat().st_size)
    jobs = []
    for i in range(count):
        artist = artists[i % len(artists)]
        title = f"{rng.choice(WORDS)} {rng.choice(NOUNS)} {i:04d}"
        year = str(1960 + (i * 7) % 66)
        genre = GENRES[(i * 5) % len(GENRES)]
        cover = cover_jpeg(f"large{i}", title, artist, size=500)
        folder = out / "large" / safe(artist) / safe(f"{year} - {title}")
        for t in range(2):
            tags = {"title": f"{title} - Part {t + 1}", "artist": artist, "album": title,
                    "album_artist": artist, "genre": genre, "date": year, "track": f"{t + 1}/2"}
            jobs.append((shortest, folder / f"{t + 1:02d} - Part {t + 1}.mp3", "mp3-copy", tags, cover))
    return jobs


# --- main ---------------------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--source", type=Path, required=True, help="directory of audio samples (read only)")
    parser.add_argument("--out", type=Path, required=True, help="where diverse/ and large/ are written")
    parser.add_argument("--large", type=int, default=2280, help="albums in large/ (0 to skip)")
    parser.add_argument("--only", choices=["diverse", "large"], help="build one library only")
    parser.add_argument("--force", action="store_true", help="rewrite existing files")
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    args = parser.parse_args()

    samples = sorted(p for p in args.source.iterdir()
                     if p.suffix.lower() in {".mp3", ".flac", ".wav", ".ogg", ".m4a"})
    if not samples:
        print(f"no audio sample in {args.source}", file=sys.stderr)
        return 1
    if args.out.resolve().is_relative_to(args.source.resolve()):
        print("--out must not be inside --source", file=sys.stderr)
        return 1
    jobs = []
    if args.only in (None, "diverse"):
        jobs += diverse_jobs(args.source, args.out, samples)
    if args.only in (None, "large") and args.large > 0:
        jobs += large_jobs(args.out, samples, args.large)

    counts = {"written": 0, "kept": 0, "failed": 0}
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(encode, *job, args.force): job for job in jobs}
        for done in concurrent.futures.as_completed(futures):
            try:
                counts[done.result()] += 1
            except subprocess.CalledProcessError as error:
                counts["failed"] += 1
                print(f"failed: {futures[done][1]}: {error}", file=sys.stderr)
    manifest = {"source": str(args.source), "files": len(jobs), **counts}
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"test library: {len(jobs)} files ({counts['written']} written, {counts['kept']} kept, "
          f"{counts['failed']} failed) in {args.out}")
    return 1 if counts["failed"] else 0


if __name__ == "__main__":
    sys.exit(main())
