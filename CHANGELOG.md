# Changelog

All notable changes to mado-news are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.4] - 2026-09-29

### Fixed
- Arrow keys, Enter and back now work when the news panel has keyboard focus: Mado sends Slint key codes (U+F700–F703, `\n`), which were not matched before

### Added
- Release workflow: tagged versions publish prebuilt binaries for macOS, Linux (x86_64 and aarch64) and Windows, so `mado plugin install news` works on every platform

## [0.1.3] - 2026-09-26

### Changed
- NerimaSoft feed (`https://nerimasoft.co.uk/feed.xml`) added to the default feed list

## [0.1.2] - 2026-09-26

### Added
- `r` keyboard shortcut to refresh feeds — avoids the mouse-click focus issue
  with the sync button; a dimmed `r` hint is shown to the left of the sync icon

### Fixed
- Sky News headlines now appear correctly — the source name lookup was matching
  against a host string that included the `https://` scheme prefix, so it never
  matched; updated to match `feeds.skynews.com` and `news-api.cf.sky.com`

## [0.1.1] - 2026-09-26

### Added
- Sync button in the header (top-right): click to force an immediate feed refresh
- Spinner icon replaces the refresh icon while a sync is in progress

### Fixed
- Sky News articles now appear — the parser now falls back to `<guid>` when
  `<link>` is not an http URL (Sky News uses `<guid>` as the article permalink)
- `news.sky.com` now shows as "Sky News" in the source label instead of "News"

## [0.1.0] - 2026-09-25

### Added
- Initial release: RSS/Atom feed reader pixel plugin for Mado sidebar
- Scrollable headline list with source labels and click-to-open in browser
- Keyboard navigation (j/k, arrow keys, Enter to open, Left to go back)
- Configurable feeds, refresh interval, and items-per-feed via `~/.config/mado/plugins/news.toml`
- Adjustable font size via `font_size` config key
- Auto-detects source name from feed URL with overrides for HN, BBC, Reuters, Ars Technica
- Nerd Font icon in header; alternating row shading; scrollbar thumb
