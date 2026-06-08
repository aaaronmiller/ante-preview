# Ante Agent Run

agent: music-auditor
backend: opencode
model: opencode-go/deepseek-v4-flash
cwd: /home/misscheta/code/ante-preview
status: exit status: 0

## Stdout

I detect **investigation intent** — inspecting a bounded music folder tree, read-only, producing a manifest of good/issue folders with follow-ups. Approach: explore → discover music folders → audit → synthesize report.

Let me start by finding all music-related directories and understanding the tree structure.
No music content in the repo root. Let me widen the search — the "bounded music-folder tree" may live elsewhere on the filesystem or within a specific subpath.
Found the music trees. The primary bounded folder is `/home/misscheta/Music`. Let me deep audit it.
Good — I can see the tree. Two major collections: `phone music 21-22` (~20k files) and `sorted-music` (~18.7k files). Let me dig deeper into both for issue detection.
Let me check a few more things — duplication between collections, organized structure patterns, and the organize_music tooling already present.
Let me get a few final metrics — the `_to_organize` backlog and total disk usage.
Comprehensive audit complete. Here is the manifest.

---

## Music Auditor Manifest — `/home/misscheta/Music`

**ANTE_AGENT_RUN_READY** | Mode: read-only inspection | Files touched: none created/edited/moved/deleted

---

### TREE OVERVIEW

```
~/Music/
├── phone music 21-22/    100 GB  20,024 files  [UNSORTED ARCHIVE — needs work]
├── sorted-music/          94 GB  18,729 files  [GOOD — mostly organized]
├── organize_music/        31 MB               [TOOLS — previous org project]
├── booger/               144 KB               [TRIVIAL — unknown content]
```

---

### ✅ GOOD FOLDERS

| Path | State | Notes |
|------|-------|-------|
| `sorted-music/` | **Organized** | 14 genre dirs (Blues, Classical, Country, Electronic, Folk, Funk, Hip-Hop, Jazz, Pop, Reggae, Rock, Soundtrack, World) + Various Artists. Artist→Album structure, 18,641 MP3s, metadata tags present on 68.4% of originals. Previous org pipeline merged 255 duplicate artist folders. |
| `organize_music/` | **Tooling** | Contains scanner.py, organizer.py, merger.py, wma_converter.py, genre_normalizer.py, mb_identifier.py — full org pipeline exists but is paused mid-WMA-conversion. |

---

### ⚠️ ISSUE FOLDERS — `phone music 21-22/`

This is the original unsorted dump. Issues per category:

**1. Unidentified Orphans — ~102 "Unknown" folders**
- `Unknown Artist/Unknown Album (date)` — **86 folders** with timestamps between 2007-2011, each a distinct dump from some device/transfer
- `Unknown/` — 2 folders (EMINEM VS SHAGGY, El Rey del Bolero Ranchero)
- Nested unknowns under named artists: `beastie boys/Unknown Album`, `Swollen Members/Unknown Album (x2)`, `pharcyde/Unknown Album`, `old dirty bastard/Unknown Album`, `sizzla/Unknown Album`, `raekwan/Unknown Album`, `hyroglyphics/Unknown Album`, `greatful dead bootleg/Unknown Album`, `zach deputy/Unknown Album`
- `liliana mix tape/Unknown Album` + `liliana mix tape/green & white mix`

**2. Explicit "Needs Sorting"**
- `music from Aaron/music SORT ME ALREADYYY/` — shouted in ALL CAPS, contains pixies + other strays

**3. Garbage/Non-Audio Debris**
- **512 `desktop.ini` files** — strewn across hundreds of folders (Windows metadata, harmless but noisy)
- **163 `.DS_Store` files** — macOS metadata
- **2 `__MACOSX/` directories** — macOS zip artifacts
- `.wmv` (15 video files), `.mid` (5 MIDI), `.ra` (1 RealAudio)

**4. Filename Hygiene Issues**
- Special chars in folder names: `FLUX.-.;',`, `droid;'[]`, `=-www.argentop2p.net=-Riddim - Buenas Noticias -2006`, `DIESEL BOY`, `DOC SCOTT`
- Release group tags in folder names: `[PMEDIA]`, `[Hunter]`, `[www.file24ever.com]`, `(by dragan09)`, `[www.torrentazos.com]`
- Inconsistent Grateful Dead bootleg naming: 17 date-coded folders (`05-21-72`, `2-25-70 ny`, etc.) but one `Unknown Album` mixed in
- Multiple deep nesting: `DJ Zeph/Dj Zeph/DJ Zeph/Sunset Scavenger` (4 levels deep)

**5. WMA Files**
- **3,324 `.wma` files** — proprietary format, lock-in risk. Conversion pipeline exists but only ~238 done.

---

### ⚠️ ISSUE FOLDERS — `sorted-music/`

**1. `_to_organize/_misc_stragglers/` — 47+ unfiled items**
These were extracted from the unsorted archive but never placed into genre/artist folders. Contains:
- Full albums (`Kendrick Lamar - DAMN.`, `TOOL - Fear Inoculum`, `NIN - Hesitation Marks`)
- Scattered singles/EPs (`Big_L-The_Danger_Zone`, `BreakScience-SevenBridges`)
- Items with messy names (`Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)`)

**2. `_to_organize/_misc_unusual/` — Non-music cruft**
20+ subdirectories by extension: `.cue`, `.dat`, `.doc`, `.html`, `.pdf`, `.rar`, `.zip`, `.torrent`, `.lnk`, `.gif`, `.mov`, `.mpg` — these are not audio files

**3. `_to_organize/_videos/` & `_to_organize/playlists/`** — video and playlist strays

**4. Genre Classification Errors**
- **Israel Vibration** (reggae/dub) filed under `Blues/`
- **Amy Winehouse** (soul/R&B) filed under `Blues/`
- `Blues/Singles/` contains mostly **Queen** tracks (rock), some Eminem — this is a dumping ground for loose files
- `Blues/Singles/` also has non-English filenames (`01 Entre Dos Aguas`, `03 Tatouage Bleu`) — multilingual mixing

**5. WMA Conversion Incomplete**
~3,086 WMA files remaining (of 3,324). Pipeline runs 4 parallel ffmpeg processes. Estimated 2-3 hours to finish at ~100/hr.

---

### RECOMMENDED FOLLOW-UP ACTIONS (ordered by impact)

| Priority | Action | Target | Effort |
|----------|--------|--------|--------|
| **P1** | Resume WMA→MP3 conversion | `sorted-music/` | Automated (~3 hrs) |
| **P2** | Identify & tag ~102 "Unknown" folders | `phone music 21-22/` | High (use MusicBrainz Picard/AcoustID) |
| **P3** | File stragglers from `_misc_stragglers/` | `sorted-music/` | Medium (47 items, mostly known artists) |
| **P4** | Purge `desktop.ini`, `.DS_Store`, `__MACOSX` | Both folders | Trivial (automated) |
| **P5** | Purge `_misc_unusual/` non-music debris | `sorted-music/` | Trivial (delete-by-extension) |
| **P6** | Move Israel Vibration → `Reggae/`, Amy Winehouse → `Pop/` or new `Soul/` | `sorted-music/` | Low |
| **P7** | File `music SORT ME ALREADYYY` content | `phone music 21-22/` | Low |
| **P8** | Normalize Grateful Dead bootleg naming | `phone music 21-22/` | Low |
| **P9** | Deduplicate across `phone music 21-22/` ↔ `sorted-music/` | Both | Medium (MP3 filename overlap detected) |
| **P10** | Merge `booger/` if relevant, delete if not | `booger/` | Trivial |

---

**Summary:** 1 good folder (`sorted-music/` — 94GB organized, ~68% tagged), 1 large issue folder (`phone music 21-22/` — 100GB unsorted with ~102 unidentified orphans and 3.3k WMA files), and a sorting backlog of 47+ stragglers. Existing tooling in `organize_music/` is well-structured but incomplete (WMA conversion mid-flight). Genre cleanup needed.


## Stderr

[0m
> ​Sisyphus - Ultraworker · deepseek-v4-flash
[0m
[0m$ [0mfind . -type d -name "*music*" -o -type d -name "*Music*" -o -type d -name "*audio*" -o -type d -name "*Audio*" -o -type d -name "*song*" -o -type d -name "*Song*" -o -type d -name "*album*" -o -type d -name "*Album*" -o -type d -name "*track*" -o -type d -name "*Track*" 2>/dev/null | head -50
[0m
[0m$ [0mfind . -type f \( -name "*.mp3" -o -name "*.flac" -o -name "*.wav" -o -name "*.aac" -o -name "*.ogg" -o -name "*.wma" -o -name "*.m4a" -o -name "*.aiff" -o -name "*.alac" -o -name "*.opus" \) 2>/dev/null | head -50
[0m
[0m$ [0mexport CI=true DEBIAN_FRONTEND=noninteractive GIT_TERMINAL_PROMPT=0 GCM_INTERACTIVE=never HOMEBREW_NO_AUTO_UPDATE=1 GIT_EDITOR=: EDITOR=: VISUAL='' GIT_SEQUENCE_EDITOR=: GIT_MERGE_AUTOEDIT=no GIT_PAGER=cat PAGER=cat npm_config_yes=true PIP_NO_INPUT=1 YARN_ENABLE_IMMUTABLE_INSTALLS=false; find . -type d -name "*music*" -o -type d -name "*music*" -o -type d -name "*audio*" -o -type f \( -name "*.mp3" -o -name "*.flac" -o -name "*.wav" -o -name "*.ogg" -o -name "*.m4a" -o -name "*.opus" -o -name "*.aiff" \) 2>/dev/null | grep -v node_modules | grep -v .git | head -100
[Category+Skill Reminder]

**Built-in**: playwright, frontend-ui-ux, git-master, dev-browser, review-work, ai-slop-remover
**⚡ YOUR SKILLS (PRIORITY)**: svelte, auto-animate, testing-patterns, algorithmic-art, mcp-builder, ui-ux-pro-max, web-artifacts-builder, skill-creator (+28 more)

> User-installed skills OVERRIDE built-in defaults. ALWAYS prefer YOUR SKILLS when domain matches.

```typescript
task(category="visual-engineering", load_skills=["svelte"], run_in_background=true)
```
[0m
[0m$ [0mfind /home/misscheta -maxdepth 6 -type d \( -iname "*music*" -o -iname "*album*" -o -iname "*songs*" \) 2>/dev/null | head -30
/home/misscheta/.cache/uv/archive-v0/vkzb3CIca4GW25PRZbI8i/elevenlabs/music
/home/misscheta/Downloads/.hermes/.hermes/hermes-agent/skills/music-creation
/home/misscheta/Downloads/.hermes/.hermes/skills.bak/media/songsee
/home/misscheta/Downloads/.hermes/.hermes/skills.bak/music-creation
/home/misscheta/Downloads/agents/skills-deprecated/songsee
/home/misscheta/Downloads/AAA/DJ Shadow - Our Pathetic Age (2019) Mp3 320kbps Album [PMEDIA]
/home/misscheta/Downloads/Music
/home/misscheta/Downloads/Music/Audio Music Apps
/home/misscheta/Downloads/Music/DJ Shadow - Our Pathetic Age (2019) Mp3 320kbps Album [PMEDIA]
/home/misscheta/Downloads/Music/Music
/home/misscheta/Downloads/Music/Music/Media.localized/Automatically Add to Music.localized
/home/misscheta/Downloads/Music/Music/Media.localized/Music
/home/misscheta/Downloads/Music/Music/Music Library.musiclibrary
/home/misscheta/Downloads/Music/Music/Music Library.musiclibrary/com.apple.MusicKit
/home/misscheta/Downloads/Music/Music/Previous Libraries.localized/Music Library [2025-12-14 8.38.49].musiclibrary
/home/misscheta/Music
/home/misscheta/Music/phone music 21-22
/home/misscheta/Music/phone music 21-22/pharcyde/Unknown Album (6-13-2008 11-26-46 PM)
/home/misscheta/Music/phone music 21-22/Music
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/Unknown Album (6-16-2008 7-51-53 PM)
/home/misscheta/Music/phone music 21-22/Anugama & Sebastiano/Exotic Dance [Open Sky Music]
/home/misscheta/Music/phone music 21-22/Raffi/Singable Songs for the Very Young
/home/misscheta/Music/phone music 21-22/hyroglyphics/Unknown Album (6-18-2008 2-02-28 PM)
/home/misscheta/Music/phone music 21-22/old dirty bastard/Unknown Album (6-13-2008 9-53-18 PM)
/home/misscheta/Music/phone music 21-22/zach deputy/Unknown Album (12-12-2010 12-26-39 AM)
/home/misscheta/Music/phone music 21-22/music from Aaron
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Cut Chemist(jurassic 5 dj) live at the bonaroo music festival 2004
/home/misscheta/Music/phone music 21-22/music from Aaron/Rock/Red Hot Chili Peppers-Discography/Unpublished Songs
/home/misscheta/Music/phone music 21-22/music from Aaron/Rock/The Beatles - White Album 2
/home/misscheta/Music/phone music 21-22/music from Aaron/Rock/Pixies - 10 Albums (4AD)
[0m
[0m$ [0mrtk ls -la /home/misscheta/code/ante-preview/
.git/
.github/
.pi/
.specify/
ante-test/
archive/
crates/
docs-site/
modules/
research/
scenarios/
specs/
target/
.gitignore  728B
AGENTS.md  618B
CHANGELOG.md  16.1K
Cargo.lock  29.6K
Cargo.toml  193B
PRODUCTION_AGENT_DEPLOYMENT_NOTES.md  6.5K
README.md  14.4K
run-habor-workflow.md  4.7K

8 files, 13 dirs (5 .md, 1 .lock, 1 .gitignore, 1 .toml)
[0m
[0m$ [0mfind /home/misscheta -maxdepth 4 -type d \( -iname "*music*" -o -iname "*audio*" -o -iname "*media*" -o -iname "*sounds*" -o -iname "*podcast*" \) 2>/dev/null | head -30
/home/misscheta/.cache/libdnf5/copr:copr.fedorainfracloud.org:avengemedia:dms-c1d8fe97a0d12bfd
/home/misscheta/.cache/libdnf5/coprdep:copr.fedorainfracloud.org:avengemedia:danklinux-611dd4e1c3f0d3f9
/home/misscheta/.config/BeeperTexts/media
/home/misscheta/.config/open-whispr/audio
/home/misscheta/Downloads/.hermes/.hermes/audio_cache
/home/misscheta/Downloads/agents/skills/satirical-media-generator
/home/misscheta/Downloads/agents/skills-inactive/ml-models-audiocraft
/home/misscheta/Downloads/AAA/DJ Shadow - Our Pathetic Age (2019) Mp3 320kbps Album [PMEDIA]
/home/misscheta/Downloads/AAA/Kendrick Lamar - Mr. Morale & The Big Steppers (2022) Mp3 320kbps [PMEDIA] ⭐️
/home/misscheta/Downloads/Music
/home/misscheta/Downloads/Music/Audio Music Apps
/home/misscheta/Downloads/Music/DJ Shadow - Our Pathetic Age (2019) Mp3 320kbps Album [PMEDIA]
/home/misscheta/Downloads/Music/iTunes/iTunes Media
/home/misscheta/Downloads/Music/Music
/home/misscheta/Downloads/Music/Music/Media.localized
/home/misscheta/Downloads/Music/Music/Music Library.musiclibrary
/home/misscheta/Music
/home/misscheta/Music/phone music 21-22
/home/misscheta/Music/phone music 21-22/Music
/home/misscheta/Music/phone music 21-22/Anugama & Sebastiano/Exotic Dance [Open Sky Music]
/home/misscheta/Music/phone music 21-22/goldie   dmz&bass/incredible sounds of dmz&bass
/home/misscheta/Music/phone music 21-22/music from Aaron
/home/misscheta/Music/phone music 21-22/music from Aaron/music SORT ME ALREADYYY
/home/misscheta/Music/phone music 21-22/Neil Young/The Music Hall, Cincinnati, OH, 2-25-1970
/home/misscheta/Music/phone music 21-22/DJ Shadow - Our Pathetic Age (2019) Mp3 320kbps Album [PMEDIA]
/home/misscheta/Music/phone music 21-22/GRiZ Music Pack
/home/misscheta/Music/phone music 21-22/Kendrick Lamar - Mr. Morale & The Big Steppers (2022) Mp3 320kbps [PMEDIA] ⭐️
/home/misscheta/Music/sorted-music
/home/misscheta/Music/sorted-music/Various Artists/Music
/home/misscheta/Music/sorted-music/Rock/Audioslave - Self Titled
[0m
[0m$ [0mfind /home/misscheta/Music -maxdepth 4 -type d | sort | grep -v '/\.'
/home/misscheta/Music
/home/misscheta/Music/booger
/home/misscheta/Music/organize_music
/home/misscheta/Music/organize_music/agent_outputs
/home/misscheta/Music/organize_music/audit-trees
/home/misscheta/Music/organize_music/music-agent
/home/misscheta/Music/organize_music/music-agent/agents
/home/misscheta/Music/organize_music/music-agent/scripts
/home/misscheta/Music/organize_music/music-agent/skills
/home/misscheta/Music/organize_music/music-agent/skills/music-organizer
/home/misscheta/Music/organize_music/trees
/home/misscheta/Music/phone music 21-22
/home/misscheta/Music/phone music 21-22/1985 - Alpha Blondy - Apartheid is Nazism
/home/misscheta/Music/phone music 21-22/2002-Sizzla-Ghetto Revolution-Sept,24
/home/misscheta/Music/phone music 21-22/40 Oz. To Freedom
/home/misscheta/Music/phone music 21-22/=-www.argentop2p.net=-Riddim - Buenas Noticias -2006
/home/misscheta/Music/phone music 21-22/A_Plus_(Of_Hieroglyphics)-Pleemix_Volume_One-(Bootleg)-2005-EGO
/home/misscheta/Music/phone music 21-22/Adama Drame
/home/misscheta/Music/phone music 21-22/Adama Drame/Mandingo Drums
/home/misscheta/Music/phone music 21-22/Aerosmith
/home/misscheta/Music/phone music 21-22/Aerosmith/O, Yeah! Ultimate Aerosmith Hits Disc 2
/home/misscheta/Music/phone music 21-22/Aesop Rock
/home/misscheta/Music/phone music 21-22/Aesop Rock/Float
/home/misscheta/Music/phone music 21-22/Aesop Rock/Labor Days
/home/misscheta/Music/phone music 21-22/Aesop Rock/None Shall Pass
/home/misscheta/Music/phone music 21-22/African Herbsman
/home/misscheta/Music/phone music 21-22/Afro Celt Sound System
/home/misscheta/Music/phone music 21-22/Afro Celt Sound System/Volume 1- Sound Magic
/home/misscheta/Music/phone music 21-22/Afrobrothers - Dejando Huella
/home/misscheta/Music/phone music 21-22/Air
/home/misscheta/Music/phone music 21-22/Air/Moon Safari
/home/misscheta/Music/phone music 21-22/Aldu Singh
/home/misscheta/Music/phone music 21-22/Anthropoworks - Anthropoworks - Annunakicks
/home/misscheta/Music/phone music 21-22/Anthropoworks - Wydow - Blue Sky & Cockroaches
/home/misscheta/Music/phone music 21-22/Antipop Consortium
/home/misscheta/Music/phone music 21-22/Antipop Consortium/Tragic Epilogue
/home/misscheta/Music/phone music 21-22/Anugama & Sebastiano
/home/misscheta/Music/phone music 21-22/Anugama & Sebastiano/Exotic Dance [Open Sky Music]
/home/misscheta/Music/phone music 21-22/Apocalyptica
/home/misscheta/Music/phone music 21-22/Apocalyptica/Plays Metallica by Four Cellos
/home/misscheta/Music/phone music 21-22/Aswad
/home/misscheta/Music/phone music 21-22/Aswad/A New Chapter of Dub
/home/misscheta/Music/phone music 21-22/Atmosphere-You_Cant_Imagine_How_Much_Fun_Were_Having-2005-CMS
/home/misscheta/Music/phone music 21-22/BILL LASWELL
/home/misscheta/Music/phone music 21-22/BILL LASWELL/ALTERED BEATS====
/home/misscheta/Music/phone music 21-22/BOX SET
/home/misscheta/Music/phone music 21-22/BREAKBEAT SCIENCE  EXERSIZE 4
/home/misscheta/Music/phone music 21-22/Babylon By Bus
/home/misscheta/Music/phone music 21-22/Bebel Gilberto
/home/misscheta/Music/phone music 21-22/Bebel Gilberto/Tanto Tempo
/home/misscheta/Music/phone music 21-22/Beethoven's Filth
/home/misscheta/Music/phone music 21-22/Big Boi - Sir Lucious Left Foot (2010) 320 Kbps
/home/misscheta/Music/phone music 21-22/Big Boi - Vicious Lies And Dangerous Rumors [2012-Album] Deluxe WEB MP3 NimitMak SilverRG
/home/misscheta/Music/phone music 21-22/Big_L-The_Danger_Zone-2011-H3X
/home/misscheta/Music/phone music 21-22/Birdy Nam Nam
/home/misscheta/Music/phone music 21-22/Birdy Nam Nam/Birdy Nam Nam
/home/misscheta/Music/phone music 21-22/Birdy Nam Nam/Defiant Order
/home/misscheta/Music/phone music 21-22/Birdy Nam Nam/Live
/home/misscheta/Music/phone music 21-22/Birdy Nam Nam/Manual For Successful Rioting
/home/misscheta/Music/phone music 21-22/Bob Dylan
/home/misscheta/Music/phone music 21-22/Bob Dylan/Blonde on Blonde
/home/misscheta/Music/phone music 21-22/Bob Dylan/Bob Dylan's Greatest Hits
/home/misscheta/Music/phone music 21-22/Bob Dylan/Love and Theft [Bonus CD] Disc 2
/home/misscheta/Music/phone music 21-22/Bob Dylan/The Bootleg Series, Vol. 4- The 'Royal Albert Hall' Concert Disc 1
/home/misscheta/Music/phone music 21-22/Bob Dylan/The Bootleg Series, Vol. 4- The 'Royal Albert Hall' Concert Disc 2
/home/misscheta/Music/phone music 21-22/Bob Dylan/The Bootleg Series, Vol. 5- Bob Dylan Live 1975 - The Rolling Thunder Revue Disc 1
/home/misscheta/Music/phone music 21-22/Bob Dylan/The Bootleg Series, Vol. 5- Bob Dylan Live 1975 - The Rolling Thunder Revue Disc 2
/home/misscheta/Music/phone music 21-22/Bob Dylan/World Gone Wrong
/home/misscheta/Music/phone music 21-22/Bob Dylan/highway 61 revisited
/home/misscheta/Music/phone music 21-22/Bob Dylan/time out of mind
/home/misscheta/Music/phone music 21-22/Bob Dylan/together - through life
/home/misscheta/Music/phone music 21-22/Bob Marley
/home/misscheta/Music/phone music 21-22/Bob Marley & The Wailers
/home/misscheta/Music/phone music 21-22/Bob Marley & The Wailers/At His Best
/home/misscheta/Music/phone music 21-22/Bob Marley & The Wailers/Legend
/home/misscheta/Music/phone music 21-22/Bob Marley/Reggae Fever Disc 2
/home/misscheta/Music/phone music 21-22/Bob Marley/The Best of Bob Marley [Madacy Box] Disc 1
/home/misscheta/Music/phone music 21-22/BoomBaptist-The Lost Files Vol. 4 (2007-2009)
/home/misscheta/Music/phone music 21-22/BoomBaptist-The Lost Files Vol. 4 (2007-2009)/BoomBaptist-The Lost Files Vol. 4 (2007-2009)
/home/misscheta/Music/phone music 21-22/BoomBaptist-The Lost Files Vol. 4 (2007-2009)/__MACOSX
/home/misscheta/Music/phone music 21-22/BoomBaptist-The Lost Files Vol. 4 (2007-2009)/__MACOSX/BoomBaptist-The Lost Files Vol. 4 (2007-2009)
/home/misscheta/Music/phone music 21-22/Break Science
/home/misscheta/Music/phone music 21-22/Break Science/Further Than Our Eyes Can See
/home/misscheta/Music/phone music 21-22/BreakScience-SevenBridges
/home/misscheta/Music/phone music 21-22/Buck 65
/home/misscheta/Music/phone music 21-22/Buck 65/Talkin' Honky Blues
/home/misscheta/Music/phone music 21-22/Buju Banton
/home/misscheta/Music/phone music 21-22/Burnin'
/home/misscheta/Music/phone music 21-22/Bush Doctor
/home/misscheta/Music/phone music 21-22/By The Way
/home/misscheta/Music/phone music 21-22/C-Rayz Walz - We Live the Black Samurai EP
/home/misscheta/Music/phone music 21-22/Café Del Mar_ The Best Of [Disc 1]
/home/misscheta/Music/phone music 21-22/Californication
/home/misscheta/Music/phone music 21-22/Catch A Fire
/home/misscheta/Music/phone music 21-22/Catch A Fire (Jamaican Origin)
/home/misscheta/Music/phone music 21-22/Chet Baker
/home/misscheta/Music/phone music 21-22/Chet Baker/Baby Breeze
/home/misscheta/Music/phone music 21-22/Collie Buddz - Come Around [2007] [Reggae] [www.file24ever.com]
/home/misscheta/Music/phone music 21-22/Cosmic Boogie - Heyoka
/home/misscheta/Music/phone music 21-22/Cypress Hill
/home/misscheta/Music/phone music 21-22/Cypress Hill Feat. Prodigy & Twin
/home/misscheta/Music/phone music 21-22/Cypress Hill Feat. Prodigy & Twin/Till Death Do Us Part
/home/misscheta/Music/phone music 21-22/Cypress Hill Feat. Tego Calderon
/home/misscheta/Music/phone music 21-22/Cypress Hill Feat. Tego Calderon/Till Death Do Us Part
/home/misscheta/Music/phone music 21-22/Cypress Hill Feat.Tim Armstrong
/home/misscheta/Music/phone music 21-22/Cypress Hill Feat.Tim Armstrong/Till Death Do Us Part
/home/misscheta/Music/phone music 21-22/Cypress Hill/Los Grandes Exitos en Espanol
/home/misscheta/Music/phone music 21-22/DIESEL BOY
/home/misscheta/Music/phone music 21-22/DIESEL BOY/SIXTH SESS
/home/misscheta/Music/phone music 21-22/DJ FARY';'
/home/misscheta/Music/phone music 21-22/DJ MELO
/home/misscheta/Music/phone music 21-22/DJ MELO/UNDER MASS # 5
/home/misscheta/Music/phone music 21-22/DJ Shadow - Live In Manchester (2018) Mp3 (320kbps) [Hunter]
/home/misscheta/Music/phone music 21-22/DJ Shadow - Live In Manchester (2018) Mp3 (320kbps) [Hunter]/DJ Shadow - Live In Manchester - The Mountain Has Fallen Tour (Live In Manchester) (320)
/home/misscheta/Music/phone music 21-22/DJ Shadow - Our Pathetic Age (2019) Mp3 320kbps Album [PMEDIA]
/home/misscheta/Music/phone music 21-22/DJ Tiësto
/home/misscheta/Music/phone music 21-22/DJ Tiësto/Live at Innercity- Amsterdam RAI
/home/misscheta/Music/phone music 21-22/DJ Zeph
/home/misscheta/Music/phone music 21-22/DJ Zeph/Azeem
/home/misscheta/Music/phone music 21-22/DJ Zeph/Azeem/Myka Nyne with Azeem
/home/misscheta/Music/phone music 21-22/DJ Zeph/DJ_Zeph-Batidas_Latinas-Promo-2009-CMS
/home/misscheta/Music/phone music 21-22/DJ Zeph/Dj Zeph
/home/misscheta/Music/phone music 21-22/DJ Zeph/Dj Zeph/DJ Zeph
/home/misscheta/Music/phone music 21-22/DJ Zeph/Dj Zeph/Sunset Scavenger
/home/misscheta/Music/phone music 21-22/DJ Zeph/On the rocks
/home/misscheta/Music/phone music 21-22/DOC SCOTT
/home/misscheta/Music/phone music 21-22/DOC SCOTT/LOST N DMZ & BASS
/home/misscheta/Music/phone music 21-22/Danger Mouse & Jemini - Ghetto Pop Life
/home/misscheta/Music/phone music 21-22/Danger Mouse and Black Thought -- Cheat Codes
/home/misscheta/Music/phone music 21-22/Dave Matthews Band
/home/misscheta/Music/phone music 21-22/Dave Matthews Band/Everyday
/home/misscheta/Music/phone music 21-22/Days To Come
/home/misscheta/Music/phone music 21-22/Dead Prez
/home/misscheta/Music/phone music 21-22/Dead Prez/Let's Get Free
/home/misscheta/Music/phone music 21-22/Dead Prez/RBG - Revolutionary but Gangsta
/home/misscheta/Music/phone music 21-22/Deltron 3030
/home/misscheta/Music/phone music 21-22/Deltron 3030/Deltron 3030
/home/misscheta/Music/phone music 21-22/Dial 'M' for Monkey
/home/misscheta/Music/phone music 21-22/Don't try this at home mp3
/home/misscheta/Music/phone music 21-22/Donald Glaude
/home/misscheta/Music/phone music 21-22/Donald Glaude/Off the Hook
/home/misscheta/Music/phone music 21-22/Dub Incorporation - 2003 - Diversité
/home/misscheta/Music/phone music 21-22/Dub Incorporation - Dans le décor
/home/misscheta/Music/phone music 21-22/Dub Incorporation - Dans le décor/Dans Le Décor
/home/misscheta/Music/phone music 21-22/Dub Incorporation - Dans le décor/Gene Harris
/home/misscheta/Music/phone music 21-22/Dub Incorporation - Dans le décor/Gene Harris/BOB MARLEY--TRIBUTE REMIX!!
/home/misscheta/Music/phone music 21-22/Dub Incorporation - Version 1.2
/home/misscheta/Music/phone music 21-22/Dub Pistols
/home/misscheta/Music/phone music 21-22/Dub Pistols/Point Blank
/home/misscheta/Music/phone music 21-22/Eek-A-Mouse_-_At_His_Best_1998_(192kbps)-CMS
/home/misscheta/Music/phone music 21-22/El-P
/home/misscheta/Music/phone music 21-22/El-P/Collecting the Kid
/home/misscheta/Music/phone music 21-22/Eminem
/home/misscheta/Music/phone music 21-22/Eminem/The Slim Shady LP
/home/misscheta/Music/phone music 21-22/Enya
/home/misscheta/Music/phone music 21-22/Enya/Amarantine
/home/misscheta/Music/phone music 21-22/Equal Rights
/home/misscheta/Music/phone music 21-22/Erykah Badu
/home/misscheta/Music/phone music 21-22/Erykah Badu/Baduizm
/home/misscheta/Music/phone music 21-22/Everything But the Girl
/home/misscheta/Music/phone music 21-22/Everything But the Girl/Back to Mine
/home/misscheta/Music/phone music 21-22/Exodus
/home/misscheta/Music/phone music 21-22/FLUX.-.;',
/home/misscheta/Music/phone music 21-22/FLUX.-.;',/LOVE SPIRALS BACKWARDS
/home/misscheta/Music/phone music 21-22/Fat Freddy's Drop
/home/misscheta/Music/phone music 21-22/Foster the People
/home/misscheta/Music/phone music 21-22/Fugees (Refugee Camp)
/home/misscheta/Music/phone music 21-22/Fugees (Refugee Camp)/The Score
/home/misscheta/Music/phone music 21-22/Further Than Our Eyes Can See
/home/misscheta/Music/phone music 21-22/GRiZ Music Pack
/home/misscheta/Music/phone music 21-22/GRiZ Music Pack/End of The World Party
/home/misscheta/Music/phone music 21-22/GRiZ Music Pack/Mad Liberation
/home/misscheta/Music/phone music 21-22/George Winston
/home/misscheta/Music/phone music 21-22/George Winston/December
/home/misscheta/Music/phone music 21-22/Get Rich Or Die Tryin'
/home/misscheta/Music/phone music 21-22/Grateful Dead
/home/misscheta/Music/phone music 21-22/Grateful Dead/American Beauty
/home/misscheta/Music/phone music 21-22/Grateful Dead/Hundred Year Hall- 4-26-72 Disc 2
/home/misscheta/Music/phone music 21-22/Grateful Dead/Reckoning
/home/misscheta/Music/phone music 21-22/Grateful Dead/Terrapin Station
/home/misscheta/Music/phone music 21-22/Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)
/home/misscheta/Music/phone music 21-22/Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)/1995 - Group Home - ''Livin' Proof''
/home/misscheta/Music/phone music 21-22/Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)/1999 - Group Home - ''A Tear For The Ghetto''
/home/misscheta/Music/phone music 21-22/Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)/2008 - Group Home - ''Where Back''
/home/misscheta/Music/phone music 21-22/Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)/2010 - Group Home - ''G.U.R.U (Gifted Unlimited Rhymes Universal)''
/home/misscheta/Music/phone music 21-22/Heyoka
/home/misscheta/Music/phone music 21-22/Holly Golightly
/home/misscheta/Music/phone music 21-22/Holly Golightly/Down Gina's at 3
/home/misscheta/Music/phone music 21-22/Jack Johnson
/home/misscheta/Music/phone music 21-22/Jack Johnson/Brushfire Fairytales
/home/misscheta/Music/phone music 21-22/Jackie Green
/home/misscheta/Music/phone music 21-22/Jackie Green/Giving Up the Ghost
/home/misscheta/Music/phone music 21-22/Jackie Greene
/home/misscheta/Music/phone music 21-22/Jackie Greene/Gone Wanderin'
/home/misscheta/Music/phone music 21-22/Jackie Greene/Sweet Somewhere Bound
/home/misscheta/Music/phone music 21-22/Jackie Greene/american myth
/home/misscheta/Music/phone music 21-22/Jackie Greene/rusty nails
/home/misscheta/Music/phone music 21-22/James Brown
/home/misscheta/Music/phone music 21-22/James Brown/Great James Brown [Platinum Disc]
/home/misscheta/Music/phone music 21-22/James Brown/James Brown's Funky People, Pt. 2
/home/misscheta/Music/phone music 21-22/Janis Joplin
/home/misscheta/Music/phone music 21-22/Janis Joplin/Janis Joplin'S Greatest Hits
/home/misscheta/Music/phone music 21-22/Janis Joplin/Janis Joplin's Greatest Hits [Bonus Tracks]
/home/misscheta/Music/phone music 21-22/Jerry Garcia
/home/misscheta/Music/phone music 21-22/Jerry Garcia/Garcia [Bonus Tracks]
/home/misscheta/Music/phone music 21-22/Jerry Garcia/Run for the Roses [Bonus Tracks]
/home/misscheta/Music/phone music 21-22/Jimi Hendrix Experience
/home/misscheta/Music/phone music 21-22/Jimi Hendrix Experience/The Ultimate Experience
/home/misscheta/Music/phone music 21-22/Kendrick Lamar - DAMN. (2017) [Mp3~320kbps]
/home/misscheta/Music/phone music 21-22/Kendrick Lamar - DAMN. (2017) [Mp3~320kbps]/Kendrick Lamar - DAMN. (2017)
/home/misscheta/Music/phone music 21-22/Kendrick Lamar - Mr. Morale & The Big Steppers (2022) Mp3 320kbps [PMEDIA] ⭐️
/home/misscheta/Music/phone music 21-22/Kendrick Lamar - Mr. Morale & The Big Steppers (2022) Mp3 320kbps [PMEDIA] ⭐️/CD1
/home/misscheta/Music/phone music 21-22/Kendrick Lamar - Mr. Morale & The Big Steppers (2022) Mp3 320kbps [PMEDIA] ⭐️/CD2
/home/misscheta/Music/phone music 21-22/Kottonmouth Kings
/home/misscheta/Music/phone music 21-22/Kottonmouth Kings/High Society
/home/misscheta/Music/phone music 21-22/Kottonmouth Kings/Royal Highness
/home/misscheta/Music/phone music 21-22/Led Zeppelin
/home/misscheta/Music/phone music 21-22/Led Zeppelin/Led Zeppelin [Box Set] Disc 1
/home/misscheta/Music/phone music 21-22/Maná
/home/misscheta/Music/phone music 21-22/Maná/MTV Unplugged
/home/misscheta/Music/phone music 21-22/Medeski Martin and Wood
/home/misscheta/Music/phone music 21-22/Medeski Martin and Wood/Shack-Man
/home/misscheta/Music/phone music 21-22/Medeski, Martin & Wood
/home/misscheta/Music/phone music 21-22/Medeski, Martin & Wood/Combustication
/home/misscheta/Music/phone music 21-22/Method Man
/home/misscheta/Music/phone music 21-22/Method Man & Redman
/home/misscheta/Music/phone music 21-22/Method Man & Redman/Blackout!
/home/misscheta/Music/phone music 21-22/Method Man/Tical 2000- Judgement Day
/home/misscheta/Music/phone music 21-22/Mos Def & Talib Kweli
/home/misscheta/Music/phone music 21-22/Mos Def & Talib Kweli/Black Star
/home/misscheta/Music/phone music 21-22/Music
/home/misscheta/Music/phone music 21-22/N.E.R.D
/home/misscheta/Music/phone music 21-22/N.E.R.D/In Search Of... [Bonus CD-Rom Track]
/home/misscheta/Music/phone music 21-22/NIN - Hesitation Marks
/home/misscheta/Music/phone music 21-22/NIN - Hesitation Marks/Twilight Frequency
/home/misscheta/Music/phone music 21-22/Nas-Damian Marley
/home/misscheta/Music/phone music 21-22/Nas-Damian Marley/Distant Relatives
/home/misscheta/Music/phone music 21-22/Neil Young
/home/misscheta/Music/phone music 21-22/Neil Young/The Music Hall, Cincinnati, OH, 2-25-1970
/home/misscheta/Music/phone music 21-22/Neo
/home/misscheta/Music/phone music 21-22/Neo/Space Country
/home/misscheta/Music/phone music 21-22/New Folder
/home/misscheta/Music/phone music 21-22/NewCocoon-PostModernMechanics
/home/misscheta/Music/phone music 21-22/Nick Drake
/home/misscheta/Music/phone music 21-22/Nick Drake/Pink Moon
/home/misscheta/Music/phone music 21-22/Nirvana
/home/misscheta/Music/phone music 21-22/Nirvana/MTV Unplugged in New York
/home/misscheta/Music/phone music 21-22/Nirvana/Nirvana [Bonus Track]
/home/misscheta/Music/phone music 21-22/Otis Redding
/home/misscheta/Music/phone music 21-22/Otis Redding/The Ultimate Otis Redding
/home/misscheta/Music/phone music 21-22/Ottmar Liebert
/home/misscheta/Music/phone music 21-22/Ottmar Liebert/Nouveau Flamenco
/home/misscheta/Music/phone music 21-22/OutKast
/home/misscheta/Music/phone music 21-22/OutKast/Big Boi and Dre Present...Outkast
/home/misscheta/Music/phone music 21-22/PKE meter
/home/misscheta/Music/phone music 21-22/PKE meter/the grassy knoll
/home/misscheta/Music/phone music 21-22/Paul Duffy
/home/misscheta/Music/phone music 21-22/Paul Duffy/Blues for Romina
/home/misscheta/Music/phone music 21-22/Peter Kruder
/home/misscheta/Music/phone music 21-22/Peter Kruder/Peace Orchestra
/home/misscheta/Music/phone music 21-22/Pierre Fournier
/home/misscheta/Music/phone music 21-22/Pierre Fournier/suites pour violoncelle seul Disc 2
/home/misscheta/Music/phone music 21-22/Pink Martini
/home/misscheta/Music/phone music 21-22/Pink Martini/Sympathique [Bonus Track]
/home/misscheta/Music/phone music 21-22/Playlists
/home/misscheta/Music/phone music 21-22/Quicksilver Messenger Service-Happy Trails [@320kbps]
/home/misscheta/Music/phone music 21-22/Quicksilver Messenger Service-Happy Trails [@320kbps]/Cover
/home/misscheta/Music/phone music 21-22/Raffi
/home/misscheta/Music/phone music 21-22/Raffi/Singable Songs for the Very Young
/home/misscheta/Music/phone music 21-22/Return of the Bomb DJ vol. 3
/home/misscheta/Music/phone music 21-22/Safi's Lab - Soul Searching
/home/misscheta/Music/phone music 21-22/Sage Francis
/home/misscheta/Music/phone music 21-22/Sage Francis/A Healthy Distrust
/home/misscheta/Music/phone music 21-22/Sage Francis/Still Sick...Urine Trouble
/home/misscheta/Music/phone music 21-22/Sage Francis/dead poet live album
/home/misscheta/Music/phone music 21-22/Sage Francis/hope
/home/misscheta/Music/phone music 21-22/Shakira
/home/misscheta/Music/phone music 21-22/Shakira/Fijación Oral, Vol. 1
/home/misscheta/Music/phone music 21-22/Spoonbill
/home/misscheta/Music/phone music 21-22/Spoonbill/Megafauna
/home/misscheta/Music/phone music 21-22/Spoonbill/Nestegg
/home/misscheta/Music/phone music 21-22/Spoonbill/Zoomorphic
/home/misscheta/Music/phone music 21-22/Stan Getz-João Gilberto
/home/misscheta/Music/phone music 21-22/Stan Getz-João Gilberto/Getz-Gilberto
/home/misscheta/Music/phone music 21-22/Sublime
/home/misscheta/Music/phone music 21-22/Sublime/40 Oz. to Freedom
/home/misscheta/Music/phone music 21-22/Sublime/Acoustic- Bradley Nowell & Friends
/home/misscheta/Music/phone music 21-22/Sublime/Sublime
/home/misscheta/Music/phone music 21-22/Swollen Members
/home/misscheta/Music/phone music 21-22/Swollen Members/Black Magic
/home/misscheta/Music/phone music 21-22/Swollen Members/Unknown Album (6-19-2008 10-01-51 AM)
/home/misscheta/Music/phone music 21-22/Swollen Members/Unknown Album (6-19-2008 10-01-52 AM)
/home/misscheta/Music/phone music 21-22/TOOL - Fear Inoculum (Deluxe) (2019) Mp3 (320kbps) [Hunter]
/home/misscheta/Music/phone music 21-22/TOOL - Fear Inoculum (Deluxe) (2019) Mp3 (320kbps) [Hunter]/TOOL - Fear Inoculum (Deluxe) (2019)
/home/misscheta/Music/phone music 21-22/The Allman Brothers Band
/home/misscheta/Music/phone music 21-22/The Allman Brothers Band/Gold Disc 1
/home/misscheta/Music/phone music 21-22/The Allman Brothers Band/Gold Disc 2
/home/misscheta/Music/phone music 21-22/The Beatles
/home/misscheta/Music/phone music 21-22/The Beatles/1967-1970 Disc 2
/home/misscheta/Music/phone music 21-22/The Beatles/Please Please Me
/home/misscheta/Music/phone music 21-22/The Beatles/beatles white Album
/home/misscheta/Music/phone music 21-22/The Doors
/home/misscheta/Music/phone music 21-22/The Doors/The Best of the Doors Disc 1
/home/misscheta/Music/phone music 21-22/The Jimi Hendrix Experience
/home/misscheta/Music/phone music 21-22/The Jimi Hendrix Experience/Axis- Bold as Love
/home/misscheta/Music/phone music 21-22/The Pogues
/home/misscheta/Music/phone music 21-22/The Pogues/Very Best of the Pogues
/home/misscheta/Music/phone music 21-22/The Presidents of the United States of America
/home/misscheta/Music/phone music 21-22/The Presidents of the United States of America/The Presidents of the United States of America
/home/misscheta/Music/phone music 21-22/The Roots
/home/misscheta/Music/phone music 21-22/The Roots/The Roots Come Alive
/home/misscheta/Music/phone music 21-22/The String Cheese Incident
/home/misscheta/Music/phone music 21-22/The String Cheese Incident/On The Road 12-13-03 Seattle, WA Disc 3
/home/misscheta/Music/phone music 21-22/They Might Be Giants
/home/misscheta/Music/phone music 21-22/They Might Be Giants/Apollo 18
/home/misscheta/Music/phone music 21-22/They Might Be Giants/Severe Tire Damage
/home/misscheta/Music/phone music 21-22/Trojan Rastafari Box Set-disc 1
/home/misscheta/Music/phone music 21-22/U-Brown
/home/misscheta/Music/phone music 21-22/U-Brown/Can't Keep a Good Man Down
/home/misscheta/Music/phone music 21-22/Unknown
/home/misscheta/Music/phone music 21-22/Unknown Artist
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-17-2008 12-03-35 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-17-2008 6-23-17 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-18-2008 8-56-55 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-19-2008 1-52-13 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-19-2008 2-05-44 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-19-2008 3-01-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-30-2008 9-19-40 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-4-2011 5-20-14 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 11-40-31 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-09-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-10-39 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-14-36 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-7-2009 11-35-07 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-7-2009 2-08-58 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-8-2009 4-29-09 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-1-2007 1-07-01 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-1-2007 12-57-49 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-1-2007 3-12-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-20-2007 11-34-24 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-20-2007 7-43-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-21-2009 12-15-09 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-28-2007 6-51-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-28-2007 8-52-34 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-29-2007 11-45-15 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-30-2007 12-06-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-30-2007 9-28-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-4-2007 1-39-14 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-4-2007 12-48-58 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-6-2007 4-23-54 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-6-2007 5-38-15 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-9-2007 7-32-32 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-1-2007 7-10-57 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-11-2010 6-00-59 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-12-2010 12-26-39 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-9-2007 7-55-22 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-10-2008 3-50-27 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-2-2008 1-52-12 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-22-2008 8-31-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-13-2010 8-07-10 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-13-2010 8-07-11 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-13-2010 8-07-18 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-15-2008 11-07-08 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-21-2008 8-05-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-22-2008 11-17-53 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-6-2008 7-24-03 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (5-4-2008 5-03-17 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (5-4-2008 5-06-08 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 10-44-43 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 11-08-27 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 6-27-22 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 6-27-23 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 6-27-24 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 8-03-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 9-59-19 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-39 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-40 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-41 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-01-50 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 7-36-07 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 7-42-00 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-15-2008 7-43-20 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 11-25-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-03-20 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-13-35 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-59-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 9-00-02 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 1-44-02 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 10-14-14 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 11-46-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-18-2008 2-04-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-20-2008 10-29-46 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (7-13-2008 7-34-29 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (7-13-2008 7-34-30 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-24-2007 7-56-19 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 1-53-25 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 2-25-00 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 6-59-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/reggae various artists from aaron
/home/misscheta/Music/phone music 21-22/Unknown/EMINEM VS SHAGGY
/home/misscheta/Music/phone music 21-22/Unknown/El Rey del Bolero Ranchero
/home/misscheta/Music/phone music 21-22/Various Artists
/home/misscheta/Music/phone music 21-22/Various Artists/Electric Soul
/home/misscheta/Music/phone music 21-22/Various Artists/Latino! Latino!
/home/misscheta/Music/phone music 21-22/Various Artists/Putumayo Presents- Acoustic Brazil
/home/misscheta/Music/phone music 21-22/Various Artists/The Funky Precedent
/home/misscheta/Music/phone music 21-22/Various Artists/The History of Our World, Part 1
/home/misscheta/Music/phone music 21-22/Viktor Vaughn
/home/misscheta/Music/phone music 21-22/Viktor Vaughn/Vaudeville Villain
/home/misscheta/Music/phone music 21-22/Wu-Tang Clan
/home/misscheta/Music/phone music 21-22/Wu-Tang Clan/Legend of the Wu-Tang Clan- Wu-Tang Clan's Greatest Hits [Clean]
/home/misscheta/Music/phone music 21-22/Wu-Tang Clan/Wu-Tang Forever Disc 1
/home/misscheta/Music/phone music 21-22/arsonists
/home/misscheta/Music/phone music 21-22/arsonists/as the world burns
/home/misscheta/Music/phone music 21-22/atmosphere
/home/misscheta/Music/phone music 21-22/atmosphere/!!!!atmosphere
/home/misscheta/Music/phone music 21-22/beastie boys
/home/misscheta/Music/phone music 21-22/beastie boys/Unknown Album (1-26-2010 11-07-19 PM)
/home/misscheta/Music/phone music 21-22/black sheep
/home/misscheta/Music/phone music 21-22/black sheep/a wolf in sheeps clothing
/home/misscheta/Music/phone music 21-22/blackalicious
/home/misscheta/Music/phone music 21-22/blackalicious/blazing arrow
/home/misscheta/Music/phone music 21-22/blacklab
/home/misscheta/Music/phone music 21-22/chacademos and pliers
/home/misscheta/Music/phone music 21-22/chacademos and pliers/conciousness alert
/home/misscheta/Music/phone music 21-22/cheech & chong
/home/misscheta/Music/phone music 21-22/cheech & chong/story book
/home/misscheta/Music/phone music 21-22/chichimeca
/home/misscheta/Music/phone music 21-22/chichimeca/chichimeka xonas
/home/misscheta/Music/phone music 21-22/clinton fearon and his boogy brown band
/home/misscheta/Music/phone music 21-22/clinton fearon and his boogy brown band/system
/home/misscheta/Music/phone music 21-22/cool hand luke
/home/misscheta/Music/phone music 21-22/cool hand luke/frigidigits
/home/misscheta/Music/phone music 21-22/cool hand luke/funk me harder
/home/misscheta/Music/phone music 21-22/cream
/home/misscheta/Music/phone music 21-22/cream/line of fire
/home/misscheta/Music/phone music 21-22/cut chemist - sound of the police
/home/misscheta/Music/phone music 21-22/cyberdread - MWCD003 - Hi Fi African Drums -- Jamendo - MP3 VBR 192k - 2008.05.30 [www.jamendo.com]
/home/misscheta/Music/phone music 21-22/daft punk
/home/misscheta/Music/phone music 21-22/daft punk/discovery
/home/misscheta/Music/phone music 21-22/dj entropy
/home/misscheta/Music/phone music 21-22/dj entropy/live at carosel
/home/misscheta/Music/phone music 21-22/dj premier
/home/misscheta/Music/phone music 21-22/dj premier/live new zealand australia
/home/misscheta/Music/phone music 21-22/dj suss one
/home/misscheta/Music/phone music 21-22/djembe drums from senegal
/home/misscheta/Music/phone music 21-22/djembe drums from senegal/from ronnie
/home/misscheta/Music/phone music 21-22/dmz&bass
/home/misscheta/Music/phone music 21-22/dmz&bass/box set -=-
/home/misscheta/Music/phone music 21-22/dmz&bass/urban takeover
/home/misscheta/Music/phone music 21-22/dr. ohms
/home/misscheta/Music/phone music 21-22/dr. ohms/jorge mexico city
/home/misscheta/Music/phone music 21-22/droid;'[]
/home/misscheta/Music/phone music 21-22/droid;'[]/nyc dmz&bass
/home/misscheta/Music/phone music 21-22/drumz   bass (D)
/home/misscheta/Music/phone music 21-22/dyme def
/home/misscheta/Music/phone music 21-22/dyme def/dyme def
/home/misscheta/Music/phone music 21-22/electronica san miguel
/home/misscheta/Music/phone music 21-22/electronica san miguel/saturday at the lounge
/home/misscheta/Music/phone music 21-22/erick songs
/home/misscheta/Music/phone music 21-22/erick songs/James Brown
/home/misscheta/Music/phone music 21-22/erick songs/James Brown/In the Jungle Groove [Bonus Track]
/home/misscheta/Music/phone music 21-22/erick songs/SOUL JAH PICKS
/home/misscheta/Music/phone music 21-22/goldie   dmz&bass
/home/misscheta/Music/phone music 21-22/goldie   dmz&bass/incredible sounds of dmz&bass
/home/misscheta/Music/phone music 21-22/goldie  various
/home/misscheta/Music/phone music 21-22/goldie  various/pure dmz&bass 2
/home/misscheta/Music/phone music 21-22/greatful dead bootleg
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/05-21-72
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/07-18-76
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/07-25-74
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/09-16-78
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/10-16-81
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/10-28-89
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/11-11-71
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/11-11-71  ...2
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-1-73
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-2-71
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-31-90
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-9-79
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/2-05-70
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/2-19-82
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/2-25-66
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/2-25-70 ny
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/4-25-77
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/7-18-76
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/8-21-72
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/Unknown Album (6-16-2008 7-51-53 PM)
/home/misscheta/Music/phone music 21-22/happy birthday inga 25!
/home/misscheta/Music/phone music 21-22/happy birthday inga 25!/from aaron
/home/misscheta/Music/phone music 21-22/high and mighty
/home/misscheta/Music/phone music 21-22/high and mighty/home field advantage
/home/misscheta/Music/phone music 21-22/high fidelity
/home/misscheta/Music/phone music 21-22/high fidelity/dub reggae
/home/misscheta/Music/phone music 21-22/hip hop
/home/misscheta/Music/phone music 21-22/hip hop/hiphop
/home/misscheta/Music/phone music 21-22/hyroglyphics
/home/misscheta/Music/phone music 21-22/hyroglyphics/Unknown Album (6-18-2008 2-02-28 PM)
/home/misscheta/Music/phone music 21-22/inga mix 2004
/home/misscheta/Music/phone music 21-22/inga mix 2004/from aaron
/home/misscheta/Music/phone music 21-22/lil fish mp3
/home/misscheta/Music/phone music 21-22/liliana electronica psycho
/home/misscheta/Music/phone music 21-22/liliana electronica psycho/lili
/home/misscheta/Music/phone music 21-22/liliana mix tape
/home/misscheta/Music/phone music 21-22/liliana mix tape/Unknown Album (6-15-2008 7-05-10 PM)
/home/misscheta/Music/phone music 21-22/liliana mix tape/green & white mix
/home/misscheta/Music/phone music 21-22/liliana purple
/home/misscheta/Music/phone music 21-22/liliana purple/from lili
/home/misscheta/Music/phone music 21-22/marly streep and george winston
/home/misscheta/Music/phone music 21-22/marly streep and george winston/the velveteen rabbit
/home/misscheta/Music/phone music 21-22/mathieu royer
/home/misscheta/Music/phone music 21-22/mathieu royer/escape from earth
/home/misscheta/Music/phone music 21-22/mos def
/home/misscheta/Music/phone music 21-22/mos def/the new danger
/home/misscheta/Music/phone music 21-22/music from Aaron
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/1996-12-15 - EM - The Psychonauts
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Amon Tobin - Discography
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Amon tobin
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Aphex Twin - Selected Ambient Works
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Automator
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Cut Chemist - Live @ Bonaroo 2004
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Cut Chemist - The Litmus Test
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Cut Chemist(jurassic 5 dj) live at the bonaroo music festival 2004
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Cut_Chemist-The_Audiences_Listening-2006-RNS[www.moviex.nn.cx]
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ Krush -meiso
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ Qbert - Wave Twisters
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ Quote - Crunk Juice The Mixtape (Hosted By E-40)
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ Shadow - In Tune and On Time (2004)-MP3
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ Weedim - Reggae Crunk Shit Vol 9 (A KRG-RELEASE) Mudbrick
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ_Premier-Australia_and_New_Zealand_Tour_Mixtape-2004-JCE
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DJ_Premier-Prime_Cuts_Vol._1_(Import)-2CD-2003-FTD
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Dan the Automator - A Much Better Tomorrow LP
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/DangerMouse - The mouse and the mask
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Dj Z-trip - Live in Los Angeles 2003
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Dj_Shadow_and_Automator_-_Bombay_the_Hard_Way [2000]
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Donald Glaude - Mixed Live
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Dutch Masters 94-98
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Fear Of A Black Tangent [2005]
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Fort_Minor-The_Rising_Tied-2005-XXL
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Girl Talk
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Glitch Mob - L.A.N. - Local Area Network
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Groove Salad- a nicely chilled plate of ambient beats and grooves. [SomaFM]
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Mix Master Mike - Spin Psycle
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Murs-The_End_Of_The_Beginning-2003-FTD
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Papoose - Build Or Destroy (2008) - Rap
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/RZA-Digi_Snacks-2008-C4
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Radiohead - In Rainbows
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Rob Swift - Sound Event
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/The Chillout Lounge
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/VA-X-Ray-Monsta_Mixes_2-2003-FTD
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/[monkee77] DJ Krush - Steppin' Stones-The Self Remixed Best-Lyricism
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/beat konducta- india - madlib
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/cut_chemist-the_litmus_test-2004-cms
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/dj shadow-preemptive strike
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/dj spark
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/madlib-mind_fusion_vol_1-ftd
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/madlib-mind_fusion_vol_2-jazz_funk_soul-2004-ftd
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/scratch perverts
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/All Funked Up - Vol. 1
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/Funk
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/George Clinton - Mothership
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/Ghost Dog - The Way of the Samurai
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/Parliament
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/Stevie Wonder - Innervisions
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/Stevie Wonder - Live 1973
/home/misscheta/Music/phone music 21-22/music from Aaron/Funk/va-ultimate_breaks_and_beats_1-25-soup
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/1996 - Looptroop-Fuck_A_Record_Deal-EP-1996-LPS
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/2Pac
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/50 Cent - The Massacre [www.tu-musica.net]
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/7l and esoteric - DC2- Bars Of Death
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/A Tribe Called Quest - Beats, Rhymes, and Life
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/A Tribe Called Quest - Instinctive Travels
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/A Tribe Called Quest - Midnight Marauders
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/A Tribe Called Quest - The Love Movement
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/A Tribe Called Quest - The Low End Theory [1991]
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/A tribe called quest-love something
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/ASHER ROTH - The GreenHouse Effect Vol. 1
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Aceyalone - Book of Human Language
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Aceyalone - Grade A
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Aesop Rock - Appleseed
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Aesop Rock - Float
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Aesop Rock - Labor Days
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Afroman - A Colt 45 Christmas (2006) - Rap By FEFE2003
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Afroman - Because I Got High
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Afroman - Drunk N High (2006) - Rap By FEFE2003
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Afroman - The Good Times
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Akrobatik
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Alkaholiks - Coast 2 Coast
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Alkolholiks - Likwidation
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Andre Nickatina - Discography
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Atmosphere - Ford Two
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Atmosphere - God Loves Ugly
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Atmosphere - Overcast
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Atmosphere - Seven's Travels
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Atmosphere-You_Cant_Imagine_How_Much_Fun_Were_Having-2005-CMS
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Awol One and Daddy Kev - Soul Doubt
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/B-Legit - Hemp Museum
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Basement Jaxx - Crazy Itch Radio[2006][CD+2SkidVids+Cov]
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - Hello Nasty
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - Ill Communication
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - Licensed to Ill
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - Paul's Boutique
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - Root Down
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - Theinsoundfromwayout
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - To The 5 Boroughs
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Beastie Boys - To The 5 Boroughs (2004) Rap [www.torrentazos.com]By FEFE2003
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Big L - Lifestyles Of The Poor and Dangerous
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Big L - The Big Picture
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Bigg jus
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Black Moses (1998)
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Blackalicious - Blazing Arrow
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Blackalicious - NIA
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Blackalicious-The_Craft-2005-ESC
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Blue Scholars - Bayani (2007) - Hip Hop
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Blue Scholars- Blue Scholars
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Boom Bap Project  - Circumstance Dictates
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Boot Camp Clik - The Chosen Few
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Brotha Lynch - EBK4
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Brotha Lynch Hung - Season Of Da Siccness (The Resurrection)
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Buck 65 - Synesthesia
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Busta Rhymes - Anarchy
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Busta Rhymes - Extinction Level Event
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Busta Rhymes - The Coming
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Busta Rhymes - When Disaster Strikes
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/C-Murder - The Best Of (2005) - Rap [www.torrentazos.com]
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/C-Murder - The Tru Story Continued (2006) - Rap By FEFE2003
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/CL Smooth - American Me (Retail 2006) - Hip Hop [www.torrentazos.com]
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/C_Murder-CP3_Mixtape-READNFO-(Webrip)-2007-MON_INT
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cage - Hell's Winter
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Can-i-bus - 2000 B.C
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Celly_Cel--Itz_Real_Out_Here-Advance-2005-AES
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Chali 2na - Fish Market
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Chino XL - Here to Save You All
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cloak_and_Dagga-Def_Con_Zero-2005
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Common - Cool Common Collected (2007) - Hip Hop By FEFE2003
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Control machete
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cunninglinguists - Sloppy Seconds
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cunninlynguists - Dirty_Acres-2007-C4
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cunninlynguists-Will_Rap_For_Food-(cd_retail)-2001-cms
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cypress Hill - Black Sunday
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cypress Hill - Bones Disc
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cypress Hill - Live
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cypress Hill - Los Grandes Exitos En Español
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Cypress Hill - Skull Disc
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/D12 - D12 World
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/DJ Clue - The Professional, Pt. 2
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/DJ Drama and Little Brother - Separate But Equal
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/DJ Quik - Under The Influence
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/DMX - Flesh Of My Flesh
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/DMX - It's Dark and Hell is Hot
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/Da Lench Mob - Guerillas in Tha Mist
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/De La Soul - De La Mixtape

...295519 bytes truncated...

The tool call succeeded but the output was truncated. Full output saved to: /home/misscheta/.local/share/opencode/tool-output/tool_e9dce9ad50011XN9a7i1MkoskF
Use the Task tool to have explore agent process this file with Grep and Read (with offset/limit). Do NOT read the full file yourself - delegate to save context.
[0m
[0m$ [0mfor dir in /home/misscheta/Music/*/; do
  name=$(basename "$dir")
  count=$(find "$dir" -type f \( -iname "*.mp3" -o -iname "*.flac" -o -iname "*.wav" -o -iname "*.aac" -o -iname "*.ogg" -o -iname "*.m4a" -o -iname "*.aiff" -o -iname "*.alac" -o -iname "*.opus" -o -iname "*.wma" \) 2>/dev/null | wc -l)
  echo "$count files  $name"
done
0 files  booger
0 files  organize_music
20024 files  phone music 21-22
18713 files  sorted-music
[0m
[0m$ [0mfind /home/misscheta/Music -type f ! \( -iname "*.mp3" -o -iname "*.flac" -o -iname "*.wav" -o -iname "*.aac" -o -iname "*.ogg" -o -iname "*.m4a" -o -iname "*.aiff" -o -iname "*.alac" -o -iname "*.opus" -o -iname "*.wma" -o -iname "*.jpg" -o -iname "*.jpeg" -o -iname "*.png" -o -iname "*.gif" -o -iname "*.nfo" -o -iname "*.txt" -o -iname "*.log" -o -iname "*.cue" -o -iname "*.pdf" -o -iname "*.sfv" -o -iname "*.m3u" -o -iname "*.pls" \) -not -path "*/\.*" -not -path "*/Music Library.musiclibrary/*" | head -40
/home/misscheta/Music/phone music 21-22/slug/slug/desktop.ini
/home/misscheta/Music/phone music 21-22/slug/desktop.ini
/home/misscheta/Music/phone music 21-22/clinton fearon and his boogy brown band/system/desktop.ini
/home/misscheta/Music/phone music 21-22/clinton fearon and his boogy brown band/desktop.ini
/home/misscheta/Music/phone music 21-22/Pierre Fournier/suites pour violoncelle seul Disc 2/desktop.ini
/home/misscheta/Music/phone music 21-22/Pierre Fournier/desktop.ini
/home/misscheta/Music/phone music 21-22/Method Man & Redman/Blackout!/desktop.ini
/home/misscheta/Music/phone music 21-22/Method Man & Redman/desktop.ini
/home/misscheta/Music/phone music 21-22/The Presidents of the United States of America/The Presidents of the United States of America/desktop.ini
/home/misscheta/Music/phone music 21-22/The Presidents of the United States of America/desktop.ini
/home/misscheta/Music/phone music 21-22/BILL LASWELL/ALTERED BEATS====/desktop.ini
/home/misscheta/Music/phone music 21-22/BILL LASWELL/desktop.ini
/home/misscheta/Music/phone music 21-22/oct.2nd.wpl
/home/misscheta/Music/phone music 21-22/pharcyde/plain rap/desktop.ini
/home/misscheta/Music/phone music 21-22/pharcyde/desktop.ini
/home/misscheta/Music/phone music 21-22/pharcyde/Unknown Album (6-13-2008 11-26-46 PM)/desktop.ini
/home/misscheta/Music/phone music 21-22/Jack Johnson/Brushfire Fairytales/desktop.ini
/home/misscheta/Music/phone music 21-22/Jack Johnson/desktop.ini
/home/misscheta/Music/phone music 21-22/Music/50 Cent - Wanksta.mid
/home/misscheta/Music/phone music 21-22/Music/50 Cent - Dont Push Me.mid
/home/misscheta/Music/phone music 21-22/Music/50 Cent - High All The Time.mid
/home/misscheta/Music/phone music 21-22/Music/desktop.ini
/home/misscheta/Music/phone music 21-22/Music/50 Cent - Many Men 1.mid
/home/misscheta/Music/phone music 21-22/Music/Cannibal Corpse - I Cum Blood.wmv
/home/misscheta/Music/phone music 21-22/Music/50 Cent - 21 Questions.mid
/home/misscheta/Music/phone music 21-22/Music/All New 2005! Loops And Samples For Acid Pro Fruity Loops Reason Trance Goa Psy House Rave Hardcore Drum And Bass Jungle Break Beats Freestyle Paul.ra
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/7-18-76/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/10-28-89/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/2-25-66/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/10-16-81/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/07-25-74/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/2-19-82/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/05-21-72/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-31-90/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/11-11-71/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/07-18-76/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/11-11-71  ...2/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-1-73/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/8-21-72/desktop.ini
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/12-9-79/desktop.ini
[0m
[0m$ [0mfind /home/misscheta/Music/sorted-music -maxdepth 3 -type d | sort | head -80
/home/misscheta/Music/sorted-music
/home/misscheta/Music/sorted-music/Blues
/home/misscheta/Music/sorted-music/Blues/Amy Winehouse
/home/misscheta/Music/sorted-music/Blues/Amy Winehouse/Back To Black (Deluxe Edition)
/home/misscheta/Music/sorted-music/Blues/B.B. King
/home/misscheta/Music/sorted-music/Blues/B.B. King/Blues Summit
/home/misscheta/Music/sorted-music/Blues/B.B. King/Live At The Regal
/home/misscheta/Music/sorted-music/Blues/Israel Vibration
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Dub Vibration_ Israel Vibration in Dub
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Dub Vibrations
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Forever
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Free To Move
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/I.V. D.U.B
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/On The Rock
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Pay the Piper
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Perfect Love and Understanding
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Praises
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Singles
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Unconquered People
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Vibes Alive!
/home/misscheta/Music/sorted-music/Blues/Israel Vibration/Wiss Vibes
/home/misscheta/Music/sorted-music/Blues/Jamalski
/home/misscheta/Music/sorted-music/Blues/Jamalski/Guaranteed Rewind
/home/misscheta/Music/sorted-music/Blues/Paul Duffy
/home/misscheta/Music/sorted-music/Blues/Paul Duffy/Blues for Romina
/home/misscheta/Music/sorted-music/Blues/Singles
/home/misscheta/Music/sorted-music/Blues/Singles/'39
/home/misscheta/Music/sorted-music/Blues/Singles/01 Entre Dos Aguas
/home/misscheta/Music/sorted-music/Blues/Singles/02 Troubled Girl [Spanish Version]
/home/misscheta/Music/sorted-music/Blues/Singles/03 Patiently Waiting
/home/misscheta/Music/sorted-music/Blues/Singles/03 Tatouage Bleu (Avec Chet)
/home/misscheta/Music/sorted-music/Blues/Singles/04 Gula Gula (Chilluminati Mix)
/home/misscheta/Music/sorted-music/Blues/Singles/06 High All The Time
/home/misscheta/Music/sorted-music/Blues/Singles/A-Mouse - Rude Boy Jamaican
/home/misscheta/Music/sorted-music/Blues/Singles/Action This Day
/home/misscheta/Music/sorted-music/Blues/Singles/Another One Bites The Dust
/home/misscheta/Music/sorted-music/Blues/Singles/Back Chat
/home/misscheta/Music/sorted-music/Blues/Singles/Bicycle Race
/home/misscheta/Music/sorted-music/Blues/Singles/Black Sun Impare
/home/misscheta/Music/sorted-music/Blues/Singles/Bohemian Rhapsody
/home/misscheta/Music/sorted-music/Blues/Singles/Brighton Rock
/home/misscheta/Music/sorted-music/Blues/Singles/Butchershop_rmx
/home/misscheta/Music/sorted-music/Blues/Singles/ChrisCo
/home/misscheta/Music/sorted-music/Blues/Singles/Classsical on down
/home/misscheta/Music/sorted-music/Blues/Singles/Crazy Little Thing Called Love
/home/misscheta/Music/sorted-music/Blues/Singles/Death On Two Legs
/home/misscheta/Music/sorted-music/Blues/Singles/Doin' Alright
/home/misscheta/Music/sorted-music/Blues/Singles/Don't Stop Me Now
/home/misscheta/Music/sorted-music/Blues/Singles/Dragon Attack
/home/misscheta/Music/sorted-music/Blues/Singles/Dreamer's Ball
/home/misscheta/Music/sorted-music/Blues/Singles/Fat Bottomed Girls
/home/misscheta/Music/sorted-music/Blues/Singles/Flash
/home/misscheta/Music/sorted-music/Blues/Singles/Get Down Make Love
/home/misscheta/Music/sorted-music/Blues/Singles/Get Down, Make Love
/home/misscheta/Music/sorted-music/Blues/Singles/God Save The Queen
/home/misscheta/Music/sorted-music/Blues/Singles/Great King Rat
/home/misscheta/Music/sorted-music/Blues/Singles/Guitar Solo
/home/misscheta/Music/sorted-music/Blues/Singles/Hammer To Fall
/home/misscheta/Music/sorted-music/Blues/Singles/Headlong
/home/misscheta/Music/sorted-music/Blues/Singles/I Can't Live With You (1997 'Rocks' Retake)
/home/misscheta/Music/sorted-music/Blues/Singles/I Want It All
/home/misscheta/Music/sorted-music/Blues/Singles/I'm In Love With My Car
/home/misscheta/Music/sorted-music/Blues/Singles/It's Late
/home/misscheta/Music/sorted-music/Blues/Singles/Keep Yourself Alive
/home/misscheta/Music/sorted-music/Blues/Singles/Killer Queen
/home/misscheta/Music/sorted-music/Blues/Singles/Let Me Entertain You
/home/misscheta/Music/sorted-music/Blues/Singles/Liar
/home/misscheta/Music/sorted-music/Blues/Singles/Love Of My Life
/home/misscheta/Music/sorted-music/Blues/Singles/Love Of My Life_1
/home/misscheta/Music/sorted-music/Blues/Singles/Modern Times Rock 'N' Roll
/home/misscheta/Music/sorted-music/Blues/Singles/My Fairy King
/home/misscheta/Music/sorted-music/Blues/Singles/No-One But You (Only The Good Die Young)
/home/misscheta/Music/sorted-music/Blues/Singles/Now I'm Here
/home/misscheta/Music/sorted-music/Blues/Singles/Now I'm Here (Reprise)
/home/misscheta/Music/sorted-music/Blues/Singles/Now I'm Here_1
/home/misscheta/Music/sorted-music/Blues/Singles/Ogre Battle
/home/misscheta/Music/sorted-music/Blues/Singles/One Vision
/home/misscheta/Music/sorted-music/Blues/Singles/Play The game
/home/misscheta/Music/sorted-music/Blues/Singles/Put Out The Fire
/home/misscheta/Music/sorted-music/Blues/Singles/Save Me
[0m
[0m$ [0mfind /home/misscheta/Music/phone\ music\ 21-22 -maxdepth 2 -type d | grep -i -E "unknown|new folder|sort|unsorted|misc" | sort
/home/misscheta/Music/phone music 21-22/Antipop Consortium
/home/misscheta/Music/phone music 21-22/Antipop Consortium/Tragic Epilogue
/home/misscheta/Music/phone music 21-22/New Folder
/home/misscheta/Music/phone music 21-22/Swollen Members/Unknown Album (6-19-2008 10-01-51 AM)
/home/misscheta/Music/phone music 21-22/Swollen Members/Unknown Album (6-19-2008 10-01-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown
/home/misscheta/Music/phone music 21-22/Unknown Artist
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-17-2008 12-03-35 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-17-2008 6-23-17 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-18-2008 8-56-55 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-19-2008 1-52-13 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-19-2008 2-05-44 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-19-2008 3-01-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-30-2008 9-19-40 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-4-2011 5-20-14 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 11-40-31 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-09-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-10-39 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-14-36 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-7-2009 11-35-07 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-7-2009 2-08-58 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-8-2009 4-29-09 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-1-2007 1-07-01 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-1-2007 12-57-49 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-1-2007 3-12-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-20-2007 11-34-24 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-20-2007 7-43-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-21-2009 12-15-09 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-28-2007 6-51-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-28-2007 8-52-34 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-29-2007 11-45-15 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-30-2007 12-06-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-30-2007 9-28-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-4-2007 1-39-14 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-4-2007 12-48-58 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-6-2007 4-23-54 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-6-2007 5-38-15 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-9-2007 7-32-32 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-1-2007 7-10-57 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-11-2010 6-00-59 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-12-2010 12-26-39 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (12-9-2007 7-55-22 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-10-2008 3-50-27 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-2-2008 1-52-12 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-22-2008 8-31-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-13-2010 8-07-10 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-13-2010 8-07-11 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-13-2010 8-07-18 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-15-2008 11-07-08 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-21-2008 8-05-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-22-2008 11-17-53 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-6-2008 7-24-03 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (5-4-2008 5-03-17 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (5-4-2008 5-06-08 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 10-44-43 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 11-08-27 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 6-27-22 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 6-27-23 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 6-27-24 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 8-03-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 9-59-19 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-39 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-40 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-00-41 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 12-01-50 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 7-36-07 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 7-42-00 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-15-2008 7-43-20 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 11-25-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-03-20 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-13-35 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-59-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 9-00-02 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 1-44-02 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 10-14-14 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 11-46-52 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-18-2008 2-04-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-20-2008 10-29-46 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (7-13-2008 7-34-29 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (7-13-2008 7-34-30 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-24-2007 7-56-19 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 1-53-25 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 2-25-00 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 6-59-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/reggae various artists from aaron
/home/misscheta/Music/phone music 21-22/Unknown/EMINEM VS SHAGGY
/home/misscheta/Music/phone music 21-22/Unknown/El Rey del Bolero Ranchero
/home/misscheta/Music/phone music 21-22/beastie boys/Unknown Album (1-26-2010 11-07-19 PM)
/home/misscheta/Music/phone music 21-22/greatful dead bootleg/Unknown Album (6-16-2008 7-51-53 PM)
/home/misscheta/Music/phone music 21-22/hyroglyphics/Unknown Album (6-18-2008 2-02-28 PM)
/home/misscheta/Music/phone music 21-22/liliana mix tape/Unknown Album (6-15-2008 7-05-10 PM)
/home/misscheta/Music/phone music 21-22/music from Aaron/music SORT ME ALREADYYY
/home/misscheta/Music/phone music 21-22/old dirty bastard/Unknown Album (6-13-2008 9-53-18 PM)
/home/misscheta/Music/phone music 21-22/pharcyde/Unknown Album (6-13-2008 11-26-46 PM)
/home/misscheta/Music/phone music 21-22/raekwan/Unknown Album (6-16-2008 6-43-53 AM)
/home/misscheta/Music/phone music 21-22/sizzla/Unknown Album (6-16-2008 7-08-27 AM)
/home/misscheta/Music/phone music 21-22/zach deputy/Unknown Album (12-12-2010 12-26-39 AM)
[0m
[0m$ [0mfind /home/misscheta/Music -type d -empty -not -path "*/\.*" 2>/dev/null | head -30
/home/misscheta/Music/phone music 21-22/BoomBaptist-The Lost Files Vol. 4 (2007-2009)/__MACOSX/BoomBaptist-The Lost Files Vol. 4 (2007-2009)
/home/misscheta/Music/phone music 21-22/New Folder
/home/misscheta/Music/phone music 21-22/zach deputy/Unknown Album (12-12-2010 12-26-39 AM)
/home/misscheta/Music/phone music 21-22/music from Aaron/DJ/Groove Salad- a nicely chilled plate of ambient beats and grooves. [SomaFM]/incomplete
/home/misscheta/Music/phone music 21-22/music from Aaron/Rock/Modest Mouse discography/New Folder
/home/misscheta/Music/phone music 21-22/music from Aaron/Rock/carrissas wierd-ugly but
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/The Grouch & Eligh - No More Greener Grasses - (2003) [[WSK]]/__MACOSX/No More Greener Grasses
/home/misscheta/Music/phone music 21-22/music from Aaron/music SORT ME ALREADYYY/pixies
/home/misscheta/Music/phone music 21-22/music from Aaron/Jazz/Taj Majal/Taj Majal
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-10-39 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-13-2008 9-59-19 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-29-2007 11-45-15 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-10-2008 3-50-27 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-20-2008 10-29-46 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-17-2008 12-03-35 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-20-2007 7-43-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (2-22-2008 8-31-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-22-2008 11-17-53 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-09-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 11-25-54 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (9-30-2007 1-53-25 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-16-2008 8-59-38 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (10-31-2007 9-14-36 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-14-2008 7-36-07 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (6-17-2008 1-44-02 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (11-28-2007 6-51-25 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-17-2008 6-23-17 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (3-21-2008 8-05-38 PM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (5-4-2008 5-03-17 AM)
/home/misscheta/Music/phone music 21-22/Unknown Artist/Unknown Album (1-18-2008 8-56-55 AM)
[0m
[0m$ [0mecho "=== phone music 21-22 ==="
for ext in mp3 flac wav aac ogg m4a aiff alac opus wma wmv mid ra; do
  count=$(find "/home/misscheta/Music/phone music 21-22" -type f -iname "*.${ext}" 2>/dev/null | wc -l)
  [ "$count" -gt 0 ] && echo "  .${ext}: $count files"
done
echo ""
echo "=== sorted-music ==="
for ext in mp3 flac wav aac ogg m4a aiff alac opus wma wmv mid ra; do
  count=$(find "/home/misscheta/Music/sorted-music" -type f -iname "*.${ext}" 2>/dev/null | wc -l)
  [ "$count" -gt 0 ] && echo "  .${ext}: $count files"
done
=== phone music 21-22 ===
  .mp3: 16456 files
  .flac: 44 files
  .wav: 45 files
  .ogg: 13 files
  .m4a: 142 files
  .wma: 3324 files
  .wmv: 15 files
  .mid: 5 files
  .ra: 1 files

=== sorted-music ===
  .mp3: 18641 files
  .flac: 18 files
  .wav: 2 files
  .ogg: 13 files
  .m4a: 39 files
  .wmv: 11 files
  .mid: 5 files
  .ra: 1 files
[0m
[0m$ [0mrtk ls -1 /home/misscheta/Music/sorted-music/
Blues/
Classical/
Country/
Electronic/
Folk/
Funk/
Hip-Hop/
Jazz/
Pop/
Reggae/
Rock/
Soundtrack/
Various Artists/
World/
_to_organize/

0 files, 15 dirs
[0m
[0m$ [0mfind /home/misscheta/Music/organize_music -type f -not -path "*/\.*" | head -30
/home/misscheta/Music/organize_music/scanner.py
/home/misscheta/Music/organize_music/catalog.json
/home/misscheta/Music/organize_music/issues.json
/home/misscheta/Music/organize_music/scan_stats.json
/home/misscheta/Music/organize_music/scan_report.txt
/home/misscheta/Music/organize_music/organizer.py
/home/misscheta/Music/organize_music/organization_plan.json
/home/misscheta/Music/organize_music/genre_notes.json
/home/misscheta/Music/organize_music/phase3_notes.md
/home/misscheta/Music/organize_music/executor.py
/home/misscheta/Music/organize_music/mb_identifier.py
/home/misscheta/Music/organize_music/execution_log.json
/home/misscheta/Music/organize_music/wma_converter.py
/home/misscheta/Music/organize_music/wma_conversion.log
/home/misscheta/Music/organize_music/file_mover.py
/home/misscheta/Music/organize_music/move_log.json
/home/misscheta/Music/organize_music/merger.py
/home/misscheta/Music/organize_music/STATUS.md
/home/misscheta/Music/organize_music/SESSION_NOTES.md
/home/misscheta/Music/organize_music/mb_identification_results.json
/home/misscheta/Music/organize_music/TIME_ESTIMATES.md
/home/misscheta/Music/organize_music/genre_normalizer.py
/home/misscheta/Music/organize_music/tag_priority_report.json
/home/misscheta/Music/organize_music/rmj_launcher.log
/home/misscheta/Music/organize_music/rmj_conversion.log
/home/misscheta/Music/organize_music/compilation_refiler.py
/home/misscheta/Music/organize_music/acoustid_results.json
/home/misscheta/Music/organize_music/no_genre_files.json
/home/misscheta/Music/organize_music/music-agent/skills/music-organizer/SKILL.md
/home/misscheta/Music/organize_music/music-agent/skills/music-organizer/DASHBOARD_SKILLS.md
[0m
[0m$ [0mcomm -12 \
  <(find "/home/misscheta/Music/phone music 21-22" -type f -iname "*.mp3" -printf '%f\n' | sort -u | head -500) \
  <(find "/home/misscheta/Music/sorted-music" -type f -iname "*.mp3" -printf '%f\n' | sort -u | head -500) | head -20
#2 In The Model Home Series.mp3
#51.mp3
'Til I Die.mp3
(((DIGITAL CRACK VOL 31))).mp3
(01) - Letter To The President.mp3
(01) 8 Million Stories - Intro.mp3
(01) Anti Nerd - No Animal Testing.mp3
(01) Anti-Doping Live in San Cristobal () - Track01.mp3
(01) Boom Bap Project Welcome to Seattle Mixtape (2005) - intro.mp3
(01) Clinton Fearon & Boogie Brown Band Give & Take () - Have A Little Fath (intro).mp3
(01) Decieving the Right eye of Perception - Track01.mp3
(01) Digital Underground Sons Of The P (1991) - The DFLO Shuttle.mp3
(01) Dr. Dre & LL Cool J Bulworth [The Soundtrack] (1998) - Zoom.mp3
(01) Green Apple Quick Step Reloaded (1995) - Hotel Wisconsin.mp3
(01) HalfWay Tree - Educated Fools feat. Treach, Bunny Wailer, & Bounty Killer.mp3
(01) Hawaii 6.30.1970 - Track01.mp3
(01) Ice Cube Greatest Hits (2001) - Pushin' Weight feat Mr. Shokhop.mp3
(01) In the Shadow of the Living Room - Mother You're Long Gone.mp3
(01) Inspector Grandes Exitos () - Track01.mp3
(01) It's Very Stimulating - An Introduction.mp3
[0m
[0m$ [0mecho "=== desktop.ini count ==="
find /home/misscheta/Music -name "desktop.ini" 2>/dev/null | wc -l
echo ""
echo "=== __MACOSX dirs ==="
find /home/misscheta/Music -type d -name "__MACOSX" 2>/dev/null
echo ""
echo "=== DS_Store files ==="
find /home/misscheta/Music -name ".DS_Store" 2>/dev/null | wc -l
=== desktop.ini count ===
512

=== __MACOSX dirs ===
/home/misscheta/Music/phone music 21-22/BoomBaptist-The Lost Files Vol. 4 (2007-2009)/__MACOSX
/home/misscheta/Music/phone music 21-22/music from Aaron/HipHop/The Grouch & Eligh - No More Greener Grasses - (2003) [[WSK]]/__MACOSX

=== DS_Store files ===
163
[0m
[0m$ [0mecho "=== _to_organize ==="
find "/home/misscheta/Music/sorted-music/_to_organize" -maxdepth 1 -type d | wc -l
echo "dirs (excluding .)"
find "/home/misscheta/Music/sorted-music/_to_organize" -maxdepth 1 -type f | wc -l
echo "files"
find "/home/misscheta/Music/sorted-music/_to_organize" -maxdepth 2 -type d | sort
=== _to_organize ===
5
dirs (excluding .)
0
files
/home/misscheta/Music/sorted-music/_to_organize
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/A_Plus_(Of_Hieroglyphics)-Pleemix_Volume_One-(Bootleg)-2005-EGO
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Aldu Singh
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Anthropoworks - Anthropoworks - Annunakicks
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Anthropoworks - Wydow - Blue Sky & Cockroaches
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Atmosphere-You_Cant_Imagine_How_Much_Fun_Were_Having-2005-CMS
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/BILL LASWELL
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Babylon By Bus
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Big Boi - Vicious Lies And Dangerous Rumors [2012-Album] Deluxe WEB MP3 NimitMak SilverRG
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Big_L-The_Danger_Zone-2011-H3X
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/BoomBaptist-The Lost Files Vol. 4 (2007-2009)
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/BreakScience-SevenBridges
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Burnin'
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Bush Doctor
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/By The Way
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Catch A Fire (Jamaican Origin)
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/DIESEL BOY
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/DJ Shadow - Live In Manchester (2018) Mp3 (320kbps) [Hunter]
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Danger Mouse & Jemini - Ghetto Pop Life
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Days To Come
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Dial 'M' for Monkey
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Don't try this at home mp3
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Dub Incorporation - Dans le décor
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Dub Incorporation - Version 1.2
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Equal Rights
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Exodus
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/FLUX.-.;',
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Group Home Discography @ 320 (4Albums)(RAP)(by dragan09)
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Kendrick Lamar - DAMN. (2017) [Mp3~320kbps]
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Kendrick Lamar - Mr. Morale & The Big Steppers (2022) Mp3 320kbps [PMEDIA] ⭐️
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/NIN - Hesitation Marks
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/NewCocoon-PostModernMechanics
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Quicksilver Messenger Service-Happy Trails [@320kbps]
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Safi's Lab - Soul Searching
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/TOOL - Fear Inoculum (Deluxe) (2019) Mp3 (320kbps) [Hunter]
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/Trojan Rastafari Box Set-disc 1
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/cyberdread - MWCD003 - Hi Fi African Drums -- Jamendo - MP3 VBR 192k - 2008.05.30 [www.jamendo.com]
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/droid;'[]
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/drumz   bass (D)
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/erick songs
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/goldie   dmz&bass
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/goldie  various
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/samba mapangala-2004 virunga roots, vol. 1
/home/misscheta/Music/sorted-music/_to_organize/_misc_stragglers/ziggy marley joy and blues
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/cue
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/dat
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/doc
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/fpl
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/gif
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/html
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/ini
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/lnk
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/md5
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/mid
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/mov
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/mpg
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/no_extension
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/pdf
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/pk
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/plc
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/pls
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/ra
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/rar
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/rtf
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/sfk
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/torrent
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/wpl
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/xls
/home/misscheta/Music/sorted-music/_to_organize/_misc_unusual/zip
/home/misscheta/Music/sorted-music/_to_organize/_videos
/home/misscheta/Music/sorted-music/_to_organize/playlists
[0m
[0m$ [0mrtk du -sh /home/misscheta/Music/*/ 2>/dev/null | sort -rh
100G	/home/misscheta/Music/phone music 21-22/
94G	/home/misscheta/Music/sorted-music/
31M	/home/misscheta/Music/organize_music/
144K	/home/misscheta/Music/booger/
[0m
[0m$ [0mrtk read /home/misscheta/Music/organize_music/STATUS.md 2>/dev/null | head -80
# Music Collection Organization - Summary Report

**Date:** 2026-04-12
**Directory:** `/home/misscheta/Music/sorted-music`

---

## What Was Done

### Phase 1: Scanning
- Scanned **20,024 audio files** (100GB, ~1,390 hours of music)
- Extracted metadata tags from all files
- Identified **1,361 unique artists** from tags
- **68.4%** of files had existing metadata tags
- Catalog saved to: `~/Music/organize_music/catalog.json`
- Issues logged: `~/Music/organize_music/issues.json`

### Phase 2: File Organization
- Moved **16,700 non-WMA files** into `Artist/Album/` structure
- **12,827** organized by Artist → Album
- **3,795** placed in `Various Artists/` (compilations, soundtracks, VA releases)
- **78** placed in `_to_organize/` (unknown artists, no metadata)

### Phase 3: Folder Merging
- Merged **255 duplicate artist folders** into canonical names:
  - `2Pac` + `2pac` + `2PAC` + `2 pac` + 30+ feature folders → **2Pac/**
  - `Bob Marley` + `BOB MARLEY` + `Bob marley` + `bob marley` → **Bob Marley/**
  - `Cat Stevens` + `CAT STEVENS` + `cat stevens` → **Cat Stevens/**
  - `Bob Dylan` + `BoB DyLaN` → **Bob Dylan/**
  - `Del the Funky Homosapien` + 5 bracketed variants + misspelling → **Del the Funky Homosapien/**
  - 200+ more merges (case fixes, feature/collab folder consolidation)

### Phase 4: WMA → MP3 Conversion (IN PROGRESS)
- **3,324 WMA files** being converted to MP3 (~190kbps V0 quality)
- Conversion runs in background with 4 parallel ffmpeg processes
- Original WMA files are deleted after successful conversion
- **~238 converted so far** (as of this report)
- **~3,086 remaining** — estimated ~2-3 hours to complete

---

## Current Structure

```
sorted-music/
├── 2Pac/
│   ├── 2Pacalypse Now/
│   ├── All Eyez On Me (Book 1)/
│   ├── All Eyez On Me (Book 2)/
│   ├── _Singles/
│   └── ...
├── Bob Dylan/
│   ├── Blonde on Blonde/
│   ├── Before the Flood/
│   └── ...
├── Cat Stevens/
│   ├── new masters/
│   ├── tea for the tillerman/
│   └── ...
├── Various Artists/
│   ├── Return of the DJ Vol. II/
│   ├── Six Ways To Sunday_ Original Motion Picture Soundtrack/
│   └── ...
├── _to_organize/
│   ├── unknown_albums/          (78 files needing identification)
│   └── no_artist/
└── [~1,230 more artist folders]
```

---

## What Still Needs Doing

### 1. WMA Conversion (automatic, in background)
- ~3,086 WMA files remain, converting at ~100/hour
- Process: `wma_converter.py --parallel 4`
- Check status: `find ~/Music/sorted-music -name "*.wma" | wc -l`
- When done, re-scan to catalog new MP3s: `python3 scanner.py`

### 2. Metadata & Filename Fixes (Phase 3 - Manual)
[0m

