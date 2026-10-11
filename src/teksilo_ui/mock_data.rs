//! The mocks build's startup data: a library that matches
//! `assets/screenshot.png`.
//!
//! [`screenshot_library`] is the one seam of the mocks build apart from the
//! cover pixels: everything else runs the real facade over an in-memory
//! database, which this fixture seeds (`startup::build_context`). It has:
//!
//! - **The album grid.** The David Bowie albums of the screenshot, in the
//!   order its grid shows them (artist, then year, then title): Space Oddity,
//!   The Rise and Fall of Ziggy Stardust, two Aladdin Sane and two Pinups,
//!   Stage (2017), a few 1975 to 1977 albums, Peter & The Wolf, Never Let Me
//!   Down, two Sound And Vision, Starman, Live Budokan 1992, Vampires of
//!   Humanity, London Boy, I Dig Everything, two best of bowie, three Station
//!   to Station, with plausible years and the genres Rock and Classic Rock.
//!   Albums sharing a title are different releases (other folders).
//! - **The filter column.** One or two songs of other artists for each genre
//!   the screenshot's filter lists (Alternativa & Punk, Alternative with
//!   Placebo under it, América, Balada, Bandas sonoras, Blues, Bolero,
//!   Britpop, Chamber Pop, Chanson, Christmas, Classic Rock, Classical,
//!   Coldwave, Country, Crooners, Dance, Desconocido, Disco, Dream, Drum &
//!   Bass, Electronic, and a few more after it).
//! - **The playlists.** The two system playlists the database creates,
//!   "Audoxidy" (some songs) and "Oxidy Drift" (empty), and the user playlist
//!   "David Bowie", which is the active one (`last_active_playlist_id`). Its
//!   rows group by folder, as in the screenshot: "FLAC (16bit-44.1kHz)", the
//!   ten songs of Aladdin Sane (2013 Remaster), 41:50 in all, and "CD 02",
//!   four songs of Heroes (2017 Remaster), 16:46 in all.
//! - **Technical data.** FLAC 16-bit 44.1 kHz, FLAC 24-bit 96 kHz and MP3
//!   320 kbps, with their sizes, so the detailed list's columns have content.
//!
//! Paths are deterministic and live under the fake root [`ROOT`]; no file
//! exists there. The data is a few hundred songs, so the mocks build starts at
//! once.

use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};

/// The fake root every mock path lives under.
pub const ROOT: &str = "/mock/music";

/// The artist of the screenshot.
pub const BOWIE: &str = "David Bowie";

/// The user playlist the screenshot shows, active at startup.
pub const BOWIE_PLAYLIST: &str = "David Bowie";

/// The folder of the first group of the "David Bowie" playlist: its group
/// title.
pub const FLAC_GROUP: &str = "FLAC (16bit-44.1kHz)";

/// The folder of the second group of the "David Bowie" playlist: its group
/// title.
pub const CD2_GROUP: &str = "CD 02";

/// The id the "David Bowie" playlist gets: the database creates "Audoxidy" (1)
/// and "Oxidy Drift" (2) first.
const BOWIE_PLAYLIST_ID: &str = "3";

/// How a release is encoded.
#[derive(Clone, Copy)]
struct Quality {
    format: &'static str,
    extension: &'static str,
    bit_depth: i64,
    sample_rate: i64,
    /// Average size of one second of audio, in bytes.
    bytes_per_second: f64,
    /// Whether the files carry ReplayGain tags.
    gain: bool,
}

const FLAC_16_44: Quality = Quality {
    format: "FLAC",
    extension: "flac",
    bit_depth: 16,
    sample_rate: 44_100,
    bytes_per_second: 90_000.0,
    gain: true,
};

const FLAC_24_96: Quality = Quality {
    format: "FLAC",
    extension: "flac",
    bit_depth: 24,
    sample_rate: 96_000,
    bytes_per_second: 260_000.0,
    gain: true,
};

const MP3_320: Quality = Quality {
    format: "MP3",
    extension: "mp3",
    bit_depth: 16,
    sample_rate: 44_100,
    bytes_per_second: 40_000.0,
    gain: false,
};

/// A song of a release: its title and its duration, in seconds.
type Track = (&'static str, f64);

/// A release: the songs of one folder.
struct Release {
    artist: &'static str,
    album: &'static str,
    /// The folder under the artist's, then an optional sub-folder (a disc, or
    /// the encoding).
    folder: &'static str,
    subfolder: Option<&'static str>,
    year: &'static str,
    genre: &'static str,
    quality: Quality,
    cover: bool,
    tracks: &'static [Track],
}

impl Release {
    /// The folder the release's files are in.
    fn directory(&self) -> String {
        let mut directory = format!("{ROOT}/{}/{}", self.artist, self.folder);
        if let Some(subfolder) = self.subfolder {
            directory.push('/');
            directory.push_str(subfolder);
        }
        directory
    }

    /// The path of track `number` (1-based).
    fn path(&self, number: usize, title: &str) -> String {
        format!(
            "{}/{number:02} - {title}.{}",
            self.directory(),
            self.quality.extension
        )
    }

    /// The songs of the release, in track order.
    fn songs(&self) -> Vec<FixtureSong> {
        let quality = self.quality;
        self.tracks
            .iter()
            .enumerate()
            .map(|(index, &(title, secs))| {
                let number = index + 1;
                FixtureSong {
                    genre: Some(self.genre.to_string()),
                    year: Some(self.year.to_string()),
                    track_number: Some(number.to_string()),
                    duration_secs: secs,
                    format: quality.format.to_string(),
                    bit_depth: quality.bit_depth,
                    sample_rate: quality.sample_rate,
                    size_bytes: (secs * quality.bytes_per_second).round() as i64,
                    track_gain: quality.gain.then(|| -6.0 - (number % 5) as f64 * 0.7),
                    album_gain: quality.gain.then_some(-6.5),
                    with_cover: self.cover,
                    ..FixtureSong::new(&self.path(number, title), title, self.artist, self.album)
                }
            })
            .collect()
    }
}

const fn bowie(
    album: &'static str,
    folder: &'static str,
    year: &'static str,
    genre: &'static str,
    quality: Quality,
    tracks: &'static [Track],
) -> Release {
    Release {
        artist: BOWIE,
        album,
        folder,
        subfolder: None,
        year,
        genre,
        quality,
        cover: true,
        tracks,
    }
}

/// The ten songs of Aladdin Sane (2013 Remaster) as the screenshot lists them:
/// each is its screenshot duration plus 0.4 s, which is what makes the group
/// 41:50 rather than 41:46 while every row still reads as the screenshot.
const ALADDIN_SANE: &[Track] = &[
    ("Watch That Man (2013 Remaster)", 270.4),
    ("Aladdin Sane (1913-1938-197?) (2013 Remaster)", 310.4),
    ("Drive-In Saturday (2013 Remaster)", 277.4),
    ("Panic in Detroit (2013 Remaster)", 268.4),
    ("Cracked Actor (2013 Remaster)", 182.4),
    ("Time (2013 Remaster)", 315.4),
    ("The Prettiest Star (2013 Remaster)", 211.4),
    ("Let's Spend the Night Together (2013 Remaster)", 190.4),
    ("The Jean Genie (2013 Remaster)", 248.4),
    ("Lady Grinning Soul (2013 Remaster)", 235.4),
];

/// Heroes (2017 Remaster), the disc "CD 02": 16:46 in all.
const HEROES_CD2: &[Track] = &[
    ("Beauty and the Beast (2017 Remaster)", 216.0),
    ("Joe the Lion (2017 Remaster)", 184.0),
    ("\"Heroes\" (2017 Remaster)", 367.0),
    ("Blackout (2017 Remaster)", 239.0),
];

/// The David Bowie releases, in no particular order (the grid sorts them).
fn bowie_releases() -> Vec<Release> {
    let mut releases = vec![
        bowie(
            "Space Oddity",
            "Space Oddity (2019 Mix)",
            "1969",
            "Rock",
            FLAC_24_96,
            &[
                ("Space Oddity (2019 Mix)", 318.0),
                ("Unwashed and Somewhat Slightly Dazed", 306.0),
                ("Don't Sit Down", 40.0),
                ("Letter to Hermione", 150.0),
                ("Cygnet Committee", 552.0),
                ("Janine", 199.0),
            ],
        ),
        bowie(
            "The Rise and Fall of Ziggy Stardust and the Spiders from Mars",
            "The Rise and Fall of Ziggy Stardust",
            "1972",
            "Rock",
            FLAC_16_44,
            &[
                ("Five Years", 283.0),
                ("Soul Love", 214.0),
                ("Moonage Daydream", 280.0),
                ("Starman", 254.0),
                ("It Ain't Easy", 177.0),
                ("Lady Stardust", 201.0),
                ("Star", 169.0),
                ("Hang On to Yourself", 160.0),
                ("Ziggy Stardust", 193.0),
                ("Suffragette City", 205.0),
                ("Rock 'n' Roll Suicide", 178.0),
            ],
        ),
        Release {
            subfolder: Some(FLAC_GROUP),
            ..bowie(
                "Aladdin Sane",
                "Aladdin Sane (2013 Remaster)",
                "1973",
                "Rock",
                FLAC_16_44,
                ALADDIN_SANE,
            )
        },
        bowie(
            "Aladdin Sane",
            "Aladdin Sane (1973 RCA LP)",
            "1973",
            "Rock",
            MP3_320,
            &[
                ("Watch That Man", 267.0),
                ("Aladdin Sane (1913-1938-197?)", 307.0),
                ("Drive-In Saturday", 273.0),
                ("Panic in Detroit", 265.0),
                ("Cracked Actor", 171.0),
            ],
        ),
        bowie(
            "Pinups",
            "Pinups (2015 Remaster)",
            "1973",
            "Rock",
            FLAC_16_44,
            &[
                ("Sorrow", 173.0),
                ("Don't Bring Me Down", 120.0),
                ("Shapes of Things", 147.0),
                ("Rosalyn", 140.0),
                ("Where Have All the Good Times Gone", 155.0),
            ],
        ),
        bowie(
            "Pinups",
            "Pinups (1973 RCA LP)",
            "1973",
            "Rock",
            MP3_320,
            &[
                ("Sorrow", 173.0),
                ("Friday on My Mind", 200.0),
                ("I Wish You Would", 149.0),
                ("See Emily Play", 253.0),
            ],
        ),
        bowie(
            "Stage (2017)",
            "Stage (2017 Remaster)",
            "1974",
            "Rock",
            FLAC_24_96,
            &[
                ("Hang On to Yourself (Live)", 197.0),
                ("Ziggy Stardust (Live)", 224.0),
                ("Five Years (Live)", 265.0),
                ("Soul Love (Live)", 215.0),
                ("Star (Live)", 160.0),
                ("Station to Station (Live)", 556.0),
            ],
        ),
        bowie(
            "Young Americans",
            "Young Americans",
            "1975",
            "Rock",
            FLAC_16_44,
            &[
                ("Young Americans", 311.0),
                ("Win", 284.0),
                ("Fascination", 330.0),
                ("Right", 283.0),
                ("Somebody Up There Likes Me", 296.0),
                ("Fame", 252.0),
            ],
        ),
        bowie(
            "Young Americans",
            "Young Americans (2016 Remaster)",
            "1975",
            "Rock",
            FLAC_24_96,
            &[
                ("Young Americans (2016 Remaster)", 311.0),
                ("Win (2016 Remaster)", 284.0),
                ("Fame (2016 Remaster)", 252.0),
            ],
        ),
        bowie(
            "Low",
            "Low",
            "1977",
            "Rock",
            FLAC_16_44,
            &[
                ("Speed of Life", 165.0),
                ("Breaking Glass", 112.0),
                ("What in the World", 140.0),
                ("Sound and Vision", 185.0),
                ("Always Crashing in the Same Car", 213.0),
                ("Be My Wife", 177.0),
                ("A New Career in a New Town", 175.0),
            ],
        ),
        bowie(
            "Low",
            "Low (2017 Remaster)",
            "1977",
            "Rock",
            FLAC_24_96,
            &[
                ("Speed of Life (2017 Remaster)", 165.0),
                ("Sound and Vision (2017 Remaster)", 185.0),
                ("Be My Wife (2017 Remaster)", 177.0),
            ],
        ),
        Release {
            subfolder: Some(CD2_GROUP),
            ..bowie(
                "Heroes",
                "Heroes (2017 Remaster)",
                "1977",
                "Rock",
                FLAC_24_96,
                HEROES_CD2,
            )
        },
        bowie(
            "Peter & The Wolf",
            "Peter & The Wolf",
            "1978",
            "Rock",
            FLAC_16_44,
            &[
                ("Peter and the Wolf (Part 1)", 540.0),
                ("Peter and the Wolf (Part 2)", 585.0),
                ("Carnival of the Animals", 120.0),
            ],
        ),
        bowie(
            "Never Let Me Down",
            "Never Let Me Down (2018 Remaster)",
            "1987",
            "Rock",
            FLAC_16_44,
            &[
                ("Day-In Day-Out", 255.0),
                ("Time Will Crawl", 258.0),
                ("Beat of Your Drum", 273.0),
                ("Never Let Me Down", 243.0),
                ("Zeroes", 354.0),
                ("Glass Spider", 331.0),
            ],
        ),
        bowie(
            "Sound And Vision",
            "Sound And Vision (Box)",
            "1989",
            "Classic Rock",
            FLAC_16_44,
            &[
                ("Space Oddity", 315.0),
                ("Changes", 217.0),
                ("Life on Mars?", 235.0),
                ("Oh! You Pretty Things", 191.0),
            ],
        ),
        bowie(
            "Sound And Vision",
            "Sound And Vision (Rykodisc)",
            "1989",
            "Classic Rock",
            MP3_320,
            &[
                ("Ziggy Stardust", 193.0),
                ("Suffragette City", 205.0),
                ("Rebel Rebel", 270.0),
            ],
        ),
        bowie(
            "Starman",
            "Starman",
            "1990",
            "Rock",
            MP3_320,
            &[
                ("Starman", 254.0),
                ("Moonage Daydream", 280.0),
                ("Lady Stardust", 201.0),
            ],
        ),
        bowie(
            "Live Budokan 1992",
            "Live Budokan 1992",
            "1992",
            "Rock",
            MP3_320,
            &[
                ("Station to Station (Live)", 564.0),
                ("Rebel Rebel (Live)", 275.0),
                ("Fame (Live)", 240.0),
                ("Heroes (Live)", 392.0),
            ],
        ),
        bowie(
            "Vampires of Humanity",
            "Vampires of Humanity",
            "1994",
            "Rock",
            MP3_320,
            &[
                ("The Buddha of Suburbia", 267.0),
                ("Strangers When We Meet", 297.0),
                ("Outside", 245.0),
            ],
        ),
        bowie(
            "London Boy",
            "London Boy",
            "1995",
            "Rock",
            FLAC_16_44,
            &[
                ("London Bye Ta-Ta", 163.0),
                ("The Laughing Gnome", 177.0),
                ("Love You till Tuesday", 190.0),
                ("Sell Me a Coat", 177.0),
            ],
        ),
        bowie(
            "I Dig Everything: The 1966 Pye Singles",
            "I Dig Everything (The 1966 Pye Singles)",
            "1999",
            "Rock",
            FLAC_16_44,
            &[
                ("I Dig Everything", 164.0),
                ("Can't Help Thinking About Me", 162.0),
                ("Do Anything You Say", 167.0),
                ("Rubber Band", 128.0),
            ],
        ),
        bowie(
            "Heathen",
            "Heathen",
            "2001",
            "Rock",
            FLAC_24_96,
            &[
                ("Sunday", 285.0),
                ("Cactus", 175.0),
                ("Slip Away", 365.0),
                ("Slow Burn", 280.0),
                ("Afraid", 208.0),
            ],
        ),
        bowie(
            "best of bowie",
            "best of bowie (CD)",
            "2002",
            "Rock",
            FLAC_16_44,
            &[
                ("Space Oddity", 315.0),
                ("Changes", 217.0),
                ("Starman", 254.0),
                ("Ziggy Stardust", 193.0),
                ("Suffragette City", 205.0),
                ("Rebel Rebel", 270.0),
            ],
        ),
        bowie(
            "best of bowie",
            "best of bowie (DVD-A rip)",
            "2002",
            "Rock",
            MP3_320,
            &[
                ("Young Americans", 311.0),
                ("Fame", 252.0),
                ("Golden Years", 247.0),
                ("Fashion", 290.0),
            ],
        ),
        bowie(
            "Five Years (1969-1973)",
            "Five Years 1969-1973",
            "2015",
            "Rock",
            FLAC_24_96,
            &[
                ("Space Oddity (2015 Remaster)", 318.0),
                ("The Prettiest Star (2015 Remaster)", 211.0),
                ("The Man Who Sold the World (2015 Remaster)", 237.0),
                ("Life on Mars? (2015 Remaster)", 235.0),
            ],
        ),
    ];
    for (folder, year, quality) in [
        ("Station to Station (2010 Deluxe)", "2016", FLAC_24_96),
        ("Station to Station (2016 Remaster)", "2016", FLAC_24_96),
        ("Station to Station (1976 RCA LP)", "2016", FLAC_16_44),
    ] {
        releases.push(bowie(
            "Station to Station",
            folder,
            year,
            "Rock",
            quality,
            &[
                ("Station to Station", 613.0),
                ("Golden Years", 247.0),
                ("Word on a Wing", 366.0),
                ("TVC 15", 329.0),
            ],
        ));
    }
    releases
}

/// A release of another artist, for the genre filter.
fn other(
    artist: &'static str,
    album: &'static str,
    year: &'static str,
    genre: &'static str,
    quality: Quality,
    tracks: &'static [Track],
) -> Release {
    Release {
        artist,
        album,
        folder: album,
        subfolder: None,
        year,
        genre,
        quality,
        cover: false,
        tracks,
    }
}

/// One or two songs for each genre of the screenshot's filter column, and a
/// few after it.
fn genre_releases() -> Vec<Release> {
    vec![
        other(
            "Placebo",
            "Placebo",
            "1996",
            "Alternative",
            FLAC_16_44,
            &[
                ("36 Degrees", 200.0),
                ("Teenage Angst", 160.0),
                ("Nancy Boy", 217.0),
            ],
        ),
        other(
            "Los Punkis",
            "Ruido Blanco",
            "2004",
            "Alternativa & Punk",
            MP3_320,
            &[("Calle Mayor", 143.0), ("Ruido Blanco", 171.0)],
        ),
        other(
            "The Static Choir",
            "Wired Hymns",
            "2011",
            "Alternative & Punk",
            MP3_320,
            &[("Wired Hymns", 189.0), ("Basement Light", 203.0)],
        ),
        other(
            "Mirror Season",
            "Slow Weather",
            "2009",
            "Alternative Rock",
            FLAC_16_44,
            &[("Slow Weather", 244.0), ("Glass Roads", 232.0)],
        ),
        other(
            "Los Caminantes",
            "Tierra Adentro",
            "1998",
            "América",
            MP3_320,
            &[("Tierra Adentro", 256.0), ("Camino al Sur", 241.0)],
        ),
        other(
            "Marisol Vega",
            "Cartas de Amor",
            "1993",
            "Balada",
            MP3_320,
            &[("Cartas de Amor", 262.0), ("Sin Ti", 275.0)],
        ),
        other(
            "Ennio Marchetti",
            "Cinema Paradiso Suites",
            "2002",
            "Bandas sonoras",
            FLAC_16_44,
            &[("Tema d'Amore", 214.0), ("Il Proiezionista", 189.0)],
        ),
        other(
            "Delta Slim",
            "Crossroads Dust",
            "1971",
            "Blues",
            FLAC_16_44,
            &[("Crossroads Dust", 268.0), ("Low Water Mark", 301.0)],
        ),
        other(
            "Trío Alba",
            "Boleros de Siempre",
            "1960",
            "Bolero",
            MP3_320,
            &[("Perfidia Azul", 187.0), ("Noche de Ronda", 198.0)],
        ),
        other(
            "The Pennine Set",
            "Northern Lights",
            "1995",
            "Britpop, Alternative",
            FLAC_16_44,
            &[("Northern Lights", 231.0), ("Saturday Parade", 205.0)],
        ),
        other(
            "Hollow Orchard",
            "Small Rooms",
            "2013",
            "Chamber Pop",
            FLAC_24_96,
            &[("Small Rooms", 227.0), ("Paper Violins", 199.0)],
        ),
        other(
            "Camille Rivière",
            "Rue des Roses",
            "1961",
            "Chanson",
            MP3_320,
            &[("Rue des Roses", 176.0), ("Le Dernier Métro", 193.0)],
        ),
        other(
            "Nordic Carol Choir",
            "Winter Evening",
            "2008",
            "Christmas",
            FLAC_16_44,
            &[("Silent Night", 214.0), ("Winter Evening", 187.0)],
        ),
        other(
            "Stonewall Riders",
            "Highway Gold",
            "1979",
            "Classic Rock",
            FLAC_16_44,
            &[("Highway Gold", 281.0), ("Last Train Out", 254.0)],
        ),
        other(
            "Anna Keller",
            "Piano Sonatas",
            "2016",
            "Classical",
            FLAC_24_96,
            &[
                ("Sonata in C, I. Allegro", 412.0),
                ("Sonata in C, II. Adagio", 523.0),
            ],
        ),
        other(
            "Bleak Harbour",
            "Grey Light",
            "1983",
            "Coldwave",
            FLAC_16_44,
            &[("Grey Light", 258.0), ("Frozen Docks", 273.0)],
        ),
        other(
            "Dusty Mills",
            "Back Porch Sessions",
            "2001",
            "Country",
            MP3_320,
            &[("Back Porch", 195.0), ("Dust on the Dashboard", 208.0)],
        ),
        other(
            "Vic Harmon",
            "After Hours",
            "1958",
            "Crooners",
            MP3_320,
            &[("After Hours", 181.0), ("Moonlight Lane", 174.0)],
        ),
        other(
            "Pulse Theory",
            "Floorwork",
            "2007",
            "Dance",
            MP3_320,
            &[("Floorwork", 372.0), ("Midnight Shift", 391.0)],
        ),
        other(
            "Cuarteto Anónimo",
            "Sin título",
            "1990",
            "Desconocido",
            MP3_320,
            &[("Pista 01", 205.0), ("Pista 02", 198.0)],
        ),
        other(
            "Velvet Arcade",
            "Disco Inferno Nights",
            "1978",
            "Disco",
            FLAC_16_44,
            &[("Disco Inferno Nights", 392.0), ("Glitter Floor", 344.0)],
        ),
        other(
            "Lunar Bloom",
            "Half Asleep",
            "2015",
            "Dream",
            FLAC_24_96,
            &[("Half Asleep", 248.0), ("Cloud Cover", 261.0)],
        ),
        other(
            "Rapid Fire",
            "Breakbeat Theory",
            "1999",
            "Drum & Bass",
            MP3_320,
            &[("Breakbeat Theory", 342.0), ("Low End", 357.0)],
        ),
        other(
            "Kraftwerk Echo",
            "Machine Dreams",
            "2010",
            "Electronic",
            FLAC_24_96,
            &[("Machine Dreams", 402.0), ("Pulse Width", 318.0)],
        ),
        other(
            "Willow & Reed",
            "Meadow Songs",
            "2012",
            "Folk",
            FLAC_16_44,
            &[("Meadow Songs", 216.0), ("Wild Thyme", 199.0)],
        ),
        other(
            "Quartet Bleu",
            "Late Set",
            "1964",
            "Jazz",
            FLAC_16_44,
            &[("Late Set", 421.0), ("Blue Hour", 387.0)],
        ),
        other(
            "Iron Cathedral",
            "Forge",
            "2005",
            "Metal",
            FLAC_16_44,
            &[("Forge", 301.0), ("Black Anvil", 327.0)],
        ),
        other(
            "Neon Parade",
            "Sugar Static",
            "2018",
            "Pop",
            MP3_320,
            &[("Sugar Static", 198.0), ("Dance Alone", 205.0)],
        ),
        other(
            "Otis Clay Jr.",
            "Sunday Soul",
            "1972",
            "Soul",
            FLAC_16_44,
            &[("Sunday Soul", 246.0), ("Come Home to Me", 233.0)],
        ),
    ]
}

/// The library of the mocks build; see the module documentation.
pub fn screenshot_library() -> LibraryFixture {
    let bowie = bowie_releases();
    let others = genre_releases();

    let songs_of = |release: &Release| release.songs();
    let mut fixture = LibraryFixture::new();
    for release in bowie.iter().chain(others.iter()) {
        for song in songs_of(release) {
            fixture = fixture.song(song);
        }
    }

    // The user playlist of the screenshot: ten songs of Aladdin Sane
    // (2013 Remaster), then the four songs of Heroes (2017 Remaster)'s CD 02.
    let paths_of = |album: &str, subfolder: &str| -> Vec<String> {
        bowie
            .iter()
            .filter(|release| release.album == album && release.subfolder == Some(subfolder))
            .flat_map(|release| {
                release
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(index, &(title, _))| release.path(index + 1, title))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let mut bowie_playlist = paths_of("Aladdin Sane", FLAC_GROUP);
    bowie_playlist.extend(paths_of("Heroes", CD2_GROUP));
    let bowie_playlist: Vec<&str> = bowie_playlist.iter().map(String::as_str).collect();

    // Some songs for the system playlist "Audoxidy": the first track of every
    // third release of the library.
    let audoxidy: Vec<String> = bowie
        .iter()
        .chain(others.iter())
        .enumerate()
        .filter(|(index, _)| index % 3 == 0)
        .filter_map(|(_, release)| {
            release
                .tracks
                .first()
                .map(|&(title, _)| release.path(1, title))
        })
        .collect();
    let audoxidy: Vec<&str> = audoxidy.iter().map(String::as_str).collect();

    fixture
        .playlist("Audoxidy", &audoxidy)
        .playlist("Oxidy Drift", &[])
        .playlist(BOWIE_PLAYLIST, &bowie_playlist)
        .setting("last_active_playlist_id", BOWIE_PLAYLIST_ID)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use teksilo::prelude::AsyncRuntimeHandle;

    use super::*;
    use crate::frontend::AppContext;
    use crate::frontend::commands::playlist as commands;
    use crate::frontend::test_fixtures::SeededLibrary;
    use crate::teksilo_ui::app::session::SessionState;
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::models::{AlbumListModel, LibrarySongsModel};
    use crate::teksilo_ui::test_support::run_until;

    fn context() -> (AppContext, SeededLibrary) {
        test_env::screenshot()
    }

    fn playlist_groups(ctx: &AppContext, id: i64) -> Vec<crate::db::database::PlaylistFolderGroup> {
        commands::load_playlist(ctx, id)
            .recv_blocking()
            .unwrap()
            .groups
    }

    #[test]
    fn the_library_is_a_few_hundred_songs_with_unique_paths() {
        let fixture = screenshot_library();
        let count = fixture.songs().len();
        assert!((150..=300).contains(&count), "{count} songs");
        let paths: HashSet<&str> = fixture.songs().iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths.len(), count, "paths are unique");
        assert!(paths.iter().all(|path| path.starts_with("/mock/music/")));
    }

    #[test]
    fn the_data_is_deterministic() {
        assert_eq!(screenshot_library(), screenshot_library());
    }

    #[test]
    fn the_three_playlists_are_there_in_tab_order() {
        let (ctx, seeded) = context();
        let tabs = commands::list_playlists(&ctx).recv_blocking().unwrap();
        let named: Vec<(&str, bool)> = tabs
            .iter()
            .map(|playlist| (playlist.name.as_str(), playlist.is_system))
            .collect();
        assert_eq!(
            named,
            vec![
                ("Audoxidy", true),
                ("Oxidy Drift", true),
                ("David Bowie", false)
            ]
        );
        assert_eq!(seeded.playlist("David Bowie").unwrap().id, 3);
        assert!(!seeded.playlist("Audoxidy").unwrap().song_ids.is_empty());
        assert!(seeded.playlist("Oxidy Drift").unwrap().song_ids.is_empty());
    }

    #[test]
    fn the_bowie_playlist_groups_as_in_the_screenshot() {
        let (ctx, seeded) = context();
        let groups = playlist_groups(&ctx, seeded.playlist("David Bowie").unwrap().id);
        assert_eq!(groups.len(), 2);

        let flac = &groups[0];
        assert_eq!(&*flac.folder_name, "FLAC (16bit-44.1kHz)");
        assert_eq!(flac.songs.len(), 10);
        assert_eq!(flac.total_duration.round() as u64, 41 * 60 + 50);
        assert_eq!(&*flac.songs[0].title, "Watch That Man (2013 Remaster)");
        assert_eq!(&*flac.songs[9].title, "Lady Grinning Soul (2013 Remaster)");
        let seconds: Vec<u64> = flac
            .songs
            .iter()
            .map(|s| s.duration.round() as u64)
            .collect();
        assert_eq!(
            seconds,
            vec![270, 310, 277, 268, 182, 315, 211, 190, 248, 235],
            "04:30 05:10 04:37 04:28 03:02 05:15 03:31 03:10 04:08 03:55"
        );

        let cd2 = &groups[1];
        assert_eq!(&*cd2.folder_name, "CD 02");
        assert_eq!(cd2.songs.len(), 4);
        assert_eq!(cd2.total_duration.round() as u64, 16 * 60 + 46);
        assert_eq!(&*cd2.songs[0].title, "Beauty and the Beast (2017 Remaster)");
        assert_eq!(cd2.songs[0].duration.round() as u64, 3 * 60 + 36);
    }

    #[test]
    fn the_session_restores_the_bowie_playlist() {
        let (ctx, seeded) = context();
        let runtime = AsyncRuntimeHandle::new();
        let session = SessionState::new();
        session.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || session
            .status()
            .get()
            .is_settled()));
        assert_eq!(
            session.with_cursor(|cursor| cursor.playlist_id()),
            Some(seeded.playlist("David Bowie").unwrap().id)
        );
    }

    #[test]
    fn the_system_playlists_hold_songs_and_nothing() {
        let (ctx, seeded) = context();
        let audoxidy = playlist_groups(&ctx, seeded.main_playlist_id);
        assert!(audoxidy.iter().map(|g| g.songs.len()).sum::<usize>() > 5);
        let drift = playlist_groups(&ctx, seeded.external_playlist_id);
        assert!(drift.is_empty());
    }

    #[test]
    fn the_grid_lists_the_screenshot_albums_in_the_screenshot_order() {
        let (ctx, _) = context();
        let runtime = AsyncRuntimeHandle::new();
        let source = LibrarySongsModel::new(ctx, runtime.clone());
        let albums = AlbumListModel::new(source.clone());
        source.reload();
        assert!(run_until(&runtime, TIMEOUT, || source.version().get() >= 1));
        let list = albums.list();
        let cards: Vec<_> = (0..list.len())
            .filter_map(|i| list.with_item(i, Clone::clone))
            .collect();
        let bowie: Vec<(&str, &str)> = cards
            .iter()
            .filter(|card| card.artist == BOWIE)
            .map(|card| (card.title.as_str(), card.year.as_str()))
            .collect();
        let head: Vec<&str> = bowie.iter().take(7).map(|(title, _)| *title).collect();
        assert_eq!(
            head,
            vec![
                "Space Oddity",
                "The Rise and Fall of Ziggy Stardust and the Spiders from Mars",
                "Aladdin Sane",
                "Aladdin Sane",
                "Pinups",
                "Pinups",
                "Stage (2017)",
            ],
            "the first row of the screenshot"
        );
        assert_eq!(bowie.len(), 28, "{bowie:?}");
        let count = |title: &str| bowie.iter().filter(|(t, _)| *t == title).count();
        assert_eq!(count("Station to Station"), 3);
        assert_eq!(count("best of bowie"), 2);
        assert_eq!(count("Sound And Vision"), 2);
        for wanted in [
            "Peter & The Wolf",
            "Never Let Me Down",
            "Starman",
            "Live Budokan 1992",
            "Vampires of Humanity",
            "London Boy",
            "I Dig Everything: The 1966 Pye Singles",
        ] {
            assert_eq!(count(wanted), 1, "{wanted}");
        }
        let ids: HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), cards.len(), "every card has its own id");
        assert!(
            cards
                .iter()
                .filter(|c| c.artist == BOWIE)
                .all(|c| c.cover_path.is_some())
        );
    }

    #[test]
    fn the_genre_filter_lists_the_genres_of_the_screenshot() {
        let (ctx, _) = context();
        let hierarchy = crate::frontend::commands::filters::load_hierarchy(
            &ctx,
            crate::logic::filter_index::FilterType::Genre,
        )
        .recv_blocking()
        .unwrap();
        let (tree, subfilters) = crate::logic::filter_index::build_tree(
            crate::logic::filter_index::FilterType::Genre,
            hierarchy,
        );
        let genres: Vec<&str> = tree.iter().map(|node| node.label.as_str()).collect();
        for wanted in [
            "Alternativa & Punk",
            "Alternative",
            "América",
            "Balada",
            "Bandas sonoras",
            "Blues",
            "Bolero",
            "Britpop, Alternative",
            "Chamber Pop",
            "Chanson",
            "Christmas",
            "Classic Rock",
            "Classical",
            "Coldwave",
            "Country",
            "Crooners",
            "Dance",
            "Desconocido",
            "Disco",
            "Dream",
            "Drum & Bass",
            "Electronic",
            "Rock",
        ] {
            assert!(
                genres.contains(&wanted),
                "{wanted} is missing from {genres:?}"
            );
        }
        let alternative = tree
            .iter()
            .find(|node| node.label == "Alternative")
            .unwrap();
        assert_eq!(
            alternative.children.first().map(|c| c.label.as_str()),
            Some("Placebo")
        );
        assert!(!subfilters.is_empty());
    }

    #[test]
    fn the_technical_data_is_varied() {
        let fixture = screenshot_library();
        let formats: HashSet<(String, i64, i64)> = fixture
            .songs()
            .iter()
            .map(|s| (s.format.clone(), s.bit_depth, s.sample_rate))
            .collect();
        assert!(formats.contains(&("FLAC".into(), 16, 44_100)));
        assert!(formats.contains(&("FLAC".into(), 24, 96_000)));
        assert!(formats.contains(&("MP3".into(), 16, 44_100)));
        assert!(
            fixture
                .songs()
                .iter()
                .all(|s| s.size_bytes > 0 && s.duration_secs > 0.0)
        );
        assert!(fixture.songs().iter().any(|s| s.track_gain.is_some()));
        assert!(
            fixture
                .songs()
                .iter()
                .any(|s| s.genre.as_deref() == Some("Classic Rock"))
        );
    }
}
