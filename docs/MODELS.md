# POD X3 model ID tables

Model IDs in the EffectDump block records (`+0 model`, see `PROTOCOL.md`)
index into per-type tables. The block's `+2` byte selects the effect table
(and `+3` is the category: `00` amp, `01` cab, `02` effect).

**Source:** the names and their order come from the string tables inside
`GearBox.exe` (v3.x, installed on the capture VM), which are stored in ID order.
The start of each table is anchored by IDs confirmed against GearBox's UI
(marked ✓). The **end** of each table is inferred from where the next group of
strings begins, so treat entries far past the last ✓ as likely rather than
confirmed. Some tables include models that need Line 6 model packs, and the
amp table includes the bass amps.

Known anomalies:

- **Delay (`02`)** starts with "Amp Tremolo" and "Delay" at IDs 0 and 1,
  which GearBox's delay menu does not offer. The confirmed IDs (2 = Analog
  Delay, 4 = Tube Echo) fix the table's alignment regardless.
- **FX loop block** (`0B`) has model ID `10` in every dump seen, but this
  table puts "POD X3 FX Loop" at 9 and "POD X3 VIBE" at 10. Unresolved.

## amp (category 00)

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | No Amp |  |
| 1 (`01`) | Line 6 21st Century Clean |  |
| 2 (`02`) | Line 6 Sparkle |  |
| 3 (`03`) | Line 6 Twang |  |
| 4 (`04`) | Line 6 Bayou |  |
| 5 (`05`) | Line 6 JTS-45 |  |
| 6 (`06`) | Line 6 Class A | ✓ menu |
| 7 (`07`) | Line 6 Mood |  |
| 8 (`08`) | Line 6 Spinal Puppet |  |
| 9 (`09`) | Line 6 Throttle |  |
| 10 (`0A`) | Line 6 Chemical X |  |
| 11 (`0B`) | Line 6 Purge |  |
| 12 (`0C`) | Line 6 Insane |  |
| 13 (`0D`) | Line 6 Octone |  |
| 14 (`0E`) | Line 6 Piezacoustic 1 |  |
| 15 (`0F`) | Line 6 Piezacoustic 2 |  |
| 16 (`10`) | 2001 Zen Master |  |
| 17 (`11`) | 1953 Small Tweed | ✓ menu + screen |
| 18 (`12`) | 1958 Tweed B-Man | ✓ menu + screen |
| 19 (`13`) | 1960 Tiny Tweed |  |
| 20 (`14`) | 1964 Blackface 'Lux |  |
| 21 (`15`) | 1965 Double Verb | ✓ menu |
| 22 (`16`) | 1996 Mini Double |  |
| 23 (`17`) | 1960 Gibtone Expo |  |
| 24 (`18`) | 1960 Two-Tone |  |
| 25 (`19`) | 1973 Hiway 100 |  |
| 26 (`1A`) | 1965 Plexi 45 |  |
| 27 (`1B`) | 1968 Plexi Lead 100 | ✓ 8A screen |
| 28 (`1C`) | 1968 Brit Plexi Bass 100 |  |
| 29 (`1D`) | 1968 Plexi Jump Lead |  |
| 30 (`1E`) | 1968 Plexi Variac'd |  |
| 31 (`1F`) | 1969 Brit Plexi Lead 200 |  |
| 32 (`20`) | 1990 Brit J-800 |  |
| 33 (`21`) | 1996 Brit JM Pre |  |
| 34 (`22`) | 1996 Match Chief |  |
| 35 (`23`) | 1993 Match D-30 |  |
| 36 (`24`) | 2001 Treadplate Dual |  |
| 37 (`25`) | 2001 Cali Diamond Plate |  |
| 38 (`26`) | 1985 Cali Crunch |  |
| 39 (`27`) | 1987 Jazz Clean |  |
| 40 (`28`) | 1967 Wishbook Silver 12 | ✓ 8D tone 2 screen |
| 41 (`29`) | 1993 Solo 100 Head |  |
| 42 (`2A`) | 1960s Super O |  |
| 43 (`2B`) | 1962 Super O Thunder |  |
| 44 (`2C`) | 1960 Class A-15 |  |
| 45 (`2D`) | 1967 Class A-30 Top Boost |  |
| 46 (`2E`) | Tube Instrument Preamp |  |
| 47 (`2F`) | 2002 Bomber Uber |  |
| 48 (`30`) | 2002 Bomber X-TC |  |
| 49 (`31`) | 2002 Angel P-Ball |  |
| 50 (`32`) | Line 6 Variax Acoustic |  |
| 51 (`33`) | Line 6 Super Clean |  |
| 52 (`34`) | Line 6 Super Sparkle |  |
| 53 (`35`) | Line 6 Sparkle Clean |  |
| 54 (`36`) | Line 6 Crunch |  |
| 55 (`37`) | Line 6 Smash |  |
| 56 (`38`) | Line 6 Fuzz |  |
| 57 (`39`) | Line 6 Chunk Chunk |  |
| 58 (`3A`) | Line 6 Big Bottom |  |
| 59 (`3B`) | Line 6 Treadplate | ✓ 5A tone 2 screen |
| 60 (`3C`) | Line 6 Lunatic |  |
| 61 (`3D`) | Line 6 Agro |  |
| 62 (`3E`) | 2003 Connor 50 |  |
| 63 (`3F`) | 2003 Deity Crunch |  |
| 64 (`40`) | 2003 Deity Lead |  |
| 65 (`41`) | 2003 Deity's Son |  |
| 66 (`42`) | 1963 Blackface Vibro |  |
| 67 (`43`) | 1967 Double Show |  |
| 68 (`44`) | 1972 Silverface Bass |  |
| 69 (`45`) | 1987 Brit Gain Silver J |  |
| 70 (`46`) | 1992 Brit Gain J-900 Clean |  |
| 71 (`47`) | 1992 Brit Gain J-900 Dist |  |
| 72 (`48`) | 2003 Brit Gain J-2000 |  |
| 73 (`49`) | 2002 Mississippi Criminal |  |
| 74 (`4A`) | Citrus D-30 |  |
| 75 (`4B`) | Line 6 Modern Hi Gain |  |
| 76 (`4C`) | Line 6 Boutique #1 |  |
| 77 (`4D`) | Class A-30 Fawn |  |
| 78 (`4E`) | Brit Gain 18 |  |
| 79 (`4F`) | Brit J-2000 #2 |  |
| 80 (`50`) | No Amp |  |
| 81 (`51`) | Line 6 Tube Preamp |  |
| 82 (`52`) | Line 6 Classic Jazz |  |
| 83 (`53`) | Line 6 Brit Invader |  |
| 84 (`54`) | Line 6 Super Thor |  |
| 85 (`55`) | Line 6 Frankenstein |  |
| 86 (`56`) | Line 6 Ebony Lux |  |
| 87 (`57`) | Line 6 Doppleganger |  |
| 88 (`58`) | Line 6 Sub Dub |  |
| 89 (`59`) | 1972 Amp 360 |  |
| 90 (`5A`) | 2003 Jaguar |  |
| 91 (`5B`) | 1975 Alchemist |  |
| 92 (`5C`) | 1974 Rock Classic |  |
| 93 (`5D`) | 1968 Flip Top |  |
| 94 (`5E`) | 1998 Adam and Eve |  |
| 95 (`5F`) | 1958 Tweed B-Man |  |
| 96 (`60`) | 1967 Silverface Bass |  |
| 97 (`61`) | 1964 Double Show |  |
| 98 (`62`) | 1989 Eighties |  |
| 99 (`63`) | 1973 Hiway 100 |  |
| 100 (`64`) | 1971 Hiway 200 |  |
| 101 (`65`) | 1969 British Major |  |
| 102 (`66`) | 1968 Brit Bass |  |
| 103 (`67`) | 2003 California |  |
| 104 (`68`) | 1998 Jazz Tone |  |
| 105 (`69`) | 1978 Stadium |  |
| 106 (`6A`) | 2002 Studio Tone |  |
| 107 (`6B`) | 1967 Motor City |  |
| 108 (`6C`) | 1965 Brit Class A100 |  |

## cab (category 01)

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | No Cabinet |  |
| 1 (`01`) | 1x6 1960s Super O |  |
| 2 (`02`) | 1x8 1960 Tiny Tweed |  |
| 3 (`03`) | 1x10 1959 Gibtone |  |
| 4 (`04`) | 1x10 1960 G-Brand |  |
| 5 (`05`) | 1x12 2001 Line 6 | ✓ screen |
| 6 (`06`) | 1x12 1953 Small Tweed | ✓ screen |
| 7 (`07`) | 1x12 1964 Blackface 'Lux | ✓ menu |
| 8 (`08`) | 1x12 1960 Class A-15 |  |
| 9 (`09`) | 2x2 2001 Mini T |  |
| 10 (`0A`) | 2x12 2001 Line 6 |  |
| 11 (`0B`) | 2x12 1965 Blackface | ✓ screen |
| 12 (`0C`) | 2x12 1996 Match Chief |  |
| 13 (`0D`) | 2x12 1987 Jazz Clean |  |
| 14 (`0E`) | 2x12 1967 Class A-30 |  |
| 15 (`0F`) | 4x10 2001 Line 6 |  |
| 16 (`10`) | 4x10 1958 Tweed B-Man | ✓ menu + screen |
| 17 (`11`) | 4x12 2001 Line 6 |  |
| 18 (`12`) | 4x12 1967 Green 20s |  |
| 19 (`13`) | 4x12 1968 Green 25s | ✓ 8A screen |
| 20 (`14`) | 4x12 1978 Brit Celest T-75s |  |
| 21 (`15`) | 4x12 1996 Brit Celest V-30s |  |
| 22 (`16`) | 4x12 2001 Treadplate | ✓ 5A tone 2 screen |
| 23 (`17`) | 1x12 California |  |
| 24 (`18`) | 1x15 1962 Thunder |  |
| 25 (`19`) | 2x12 Zen Master |  |
| 26 (`1A`) | 2x12 1967 Wishbook | ✓ 8D tone 2 screen |
| 27 (`1B`) | 4x12 Hiway |  |
| 28 (`1C`) | 4x12 HiGn SOLO |  |
| 29 (`1D`) | 2x10 Vibro |  |
| 30 (`1E`) | 4x12 X-Load |  |
| 31 (`1F`) | 4x12 Big Bottom |  |
| 32 (`20`) | 2x12 Fawn |  |
| 33 (`21`) | No Cabinet |  |
| 34 (`22`) | 1x12 Boutique |  |
| 35 (`23`) | 1x12 Motor City |  |
| 36 (`24`) | 1x15 Flip Top |  |
| 37 (`25`) | 1x15 Jazz Tone |  |
| 38 (`26`) | 1x18 Session |  |
| 39 (`27`) | 1x18 Amp 360 |  |
| 40 (`28`) | 1x18 California |  |
| 41 (`29`) | 1x18+12 Stadium |  |
| 42 (`2A`) | 2x10 Modern UK |  |
| 43 (`2B`) | 2x15 Doubleshow |  |
| 44 (`2C`) | 2x15 California |  |
| 45 (`2D`) | 2x15 Class A |  |
| 46 (`2E`) | 4x10 Line 6 |  |
| 47 (`2F`) | 4x10 Tweed |  |
| 48 (`30`) | 4x10 Adam and Eve |  |
| 49 (`31`) | 4x10 Silvercone |  |
| 50 (`32`) | 4x10 Session |  |
| 51 (`33`) | 4x12 Hiway |  |
| 52 (`34`) | 4x12 Green 20s |  |
| 53 (`35`) | 4x12 Green 25s |  |
| 54 (`36`) | 4x15 Big Boy |  |
| 55 (`37`) | 8x10 Classic |  |

## type 02 delay

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Amp Tremolo |  |
| 1 (`01`) | Delay |  |
| 2 (`02`) | Analog Delay | ✓ menu pick |
| 3 (`03`) | Analog Delay w/Modulation |  |
| 4 (`04`) | Tube Echo | ✓ 8D screen |
| 5 (`05`) | Multi-Head Delay |  |
| 6 (`06`) | Sweep Echo |  |
| 7 (`07`) | Digital Delay |  |
| 8 (`08`) | Stereo Delay |  |
| 9 (`09`) | Ping Pong Delay |  |
| 10 (`0A`) | Reverse Delay |  |
| 11 (`0B`) | Tape Echo |  |
| 12 (`0C`) | Echo Platter |  |
| 13 (`0D`) | Low Rez |  |
| 14 (`0E`) | Phaze Eko |  |
| 15 (`0F`) | Bubble Echo |  |
| 16 (`10`) | Ducking Delay |  |
| 17 (`11`) | Dynamic Delay |  |

## type 03 mod

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Chorus |  |
| 1 (`01`) | Flanger |  |
| 2 (`02`) | Rotary |  |
| 3 (`03`) | Tremolo |  |
| 4 (`04`) | Sine Chorus |  |
| 5 (`05`) | Analog Chorus |  |
| 6 (`06`) | Line 6 Flanger |  |
| 7 (`07`) | Jet Flanger |  |
| 8 (`08`) | Phaser |  |
| 9 (`09`) | U-Vibe |  |
| 10 (`0A`) | Opto Tremolo |  |
| 11 (`0B`) | Bias Tremolo | ✓ 8D screen |
| 12 (`0C`) | Rotary Drum + Horn |  |
| 13 (`0D`) | Rotary Drum |  |
| 14 (`0E`) | Auto Pan |  |
| 15 (`0F`) | Chorus 2 |  |
| 16 (`10`) | Flanger 2 |  |
| 17 (`11`) | Lumpy Phase |  |
| 18 (`12`) | Stereo Square Chorus |  |
| 19 (`13`) | Expo Flange |  |
| 20 (`14`) | Random Chorus |  |
| 21 (`15`) | Analog Square Chorus |  |
| 22 (`16`) | Stereo Chorus |  |
| 23 (`17`) | POD Purple X |  |
| 24 (`18`) | Random S & H |  |
| 25 (`19`) | Tape Eater |  |
| 26 (`1A`) | Hi Talk |  |
| 27 (`1B`) | Sweeper |  |
| 28 (`1C`) | Warble-Matic |  |
| 29 (`1D`) | Line 6 Stereo Flange |  |
| 30 (`1E`) | Stereo Expo Chorus |  |
| 31 (`1F`) | Stereo Square Flange |  |
| 32 (`20`) | Pitch Vibrato |  |
| 33 (`21`) | Barberpole Phaser |  |
| 34 (`22`) | Dimension |  |
| 35 (`23`) | Pattern Tremolo |  |
| 36 (`24`) | Script Phase |  |
| 37 (`25`) | Voice Box |  |
| 38 (`26`) | OctiSynth |  |
| 39 (`27`) | Vocoder |  |

## type 04 reverb

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Room Reverb |  |
| 1 (`01`) | Spring Reverb |  |
| 2 (`02`) | 'Lux Spring |  |
| 3 (`03`) | Standard Spring |  |
| 4 (`04`) | King Spring |  |
| 5 (`05`) | Small Room |  |
| 6 (`06`) | Tiled Room |  |
| 7 (`07`) | Brite Room | ✓ 8D screen |
| 8 (`08`) | Dark Hall |  |
| 9 (`09`) | Medium Hall | ✓ 5A screen |
| 10 (`0A`) | Large Hall |  |
| 11 (`0B`) | Rich Chamber |  |
| 12 (`0C`) | Chamber |  |
| 13 (`0D`) | Cavernous |  |
| 14 (`0E`) | Slap Plate |  |
| 15 (`0F`) | Vintage Plate |  |
| 16 (`10`) | Large Plate |  |
| 17 (`11`) | Large Room |  |
| 18 (`12`) | Studio 6 |  |
| 19 (`13`) | Memphis plate |  |
| 20 (`14`) | Foil plate |  |
| 21 (`15`) | Blue plate |  |
| 22 (`16`) | Bingo Hall |  |
| 23 (`17`) | Concert Hall |  |
| 24 (`18`) | War Memorial |  |
| 25 (`19`) | Hangar 18 |  |
| 26 (`1A`) | Propellar Verb |  |
| 27 (`1B`) | Radio Verb |  |

## type 05 stomp (distortion)

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Facial Fuzz |  |
| 1 (`01`) | Tube Drive |  |
| 2 (`02`) | Fuzz Pi | ✓ 8D screen |
| 3 (`03`) | Screamer |  |
| 4 (`04`) | Octave Fuzz |  |
| 5 (`05`) | Classic Distortion |  |
| 6 (`06`) | Killer Z |  |
| 7 (`07`) | Boost + EQ |  |
| 8 (`08`) | Bass Overdrive |  |
| 9 (`09`) | Bronze Master |  |

## type 06 wah

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Vetta Wah | ✓ 8D screen |
| 1 (`01`) | POD Wah |  |
| 2 (`02`) | Vetta Wah |  |
| 3 (`03`) | Fassel |  |
| 4 (`04`) | Weeper |  |
| 5 (`05`) | Chrome |  |
| 6 (`06`) | Chrome Custom |  |
| 7 (`07`) | Throaty |  |
| 8 (`08`) | Conductor |  |
| 9 (`09`) | Colorful |  |
| 10 (`0A`) | Custom |  |

## type 07 volume

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Volume | ✓ 8D screen |
| 1 (`01`) | Volume |  |
| 2 (`02`) | Volume |  |
| 3 (`03`) | Volume |  |

## type 0A stomp (filter/synth)

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Auto Wah | ✓ 8D tone 2 screen |
| 1 (`01`) | Synth Lead |  |
| 2 (`02`) | Synth String |  |
| 3 (`03`) | Synth Analog |  |
| 4 (`04`) | Synth FX |  |
| 5 (`05`) | Buzz Wave |  |
| 6 (`06`) | Rez Synth |  |
| 7 (`07`) | Saturn 5 Ring Mod |  |
| 8 (`08`) | Double Bass |  |
| 9 (`09`) | Synth Harmony |  |
| 10 (`0A`) | Dingo Tron |  |
| 11 (`0B`) | Clean Sweep |  |
| 12 (`0C`) | Seismik Synth |  |
| 13 (`0D`) | Sub Octaves |  |
| 14 (`0E`) | Bender |  |
| 15 (`0F`) | Frequency Shifter |  |
| 16 (`10`) | Q Filter |  |
| 17 (`11`) | V Tron |  |

## type 0B gate/comp/misc

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Noise Gate | ✓ gate block |
| 1 (`01`) | Compressor | ✓ comp block |
| 2 (`02`) | Vetta Noise Gate |  |
| 3 (`03`) | Vetta Amp Compressor |  |
| 4 (`04`) | Vetta AIR |  |
| 5 (`05`) | Vetta Doubletracker |  |
| 6 (`06`) | Vetta FX Loop |  |
| 7 (`07`) | Vetta Floorboard |  |
| 8 (`08`) | Vetta Pitchshifter |  |
| 9 (`09`) | POD X3 FX Loop |  |
| 10 (`0A`) | POD X3 VIBE |  |
| 11 (`0B`) | Variax Electric |  |
| 12 (`0C`) | Variax Acoustic |  |
| 13 (`0D`) | Variax Bass |  |

## type 0C EQ

| ID | Name | Confirmed |
|---|---|---|
| 0 (`00`) | Graphic EQ |  |
| 1 (`01`) | 4 Band EQ |  |
| 2 (`02`) | 4 Band SemiParametric EQ | ✓ 8D screen (4 bands, gain + freq) |
| 3 (`03`) | Bass PODxt Post EQ |  |
