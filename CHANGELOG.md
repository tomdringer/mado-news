# Changelog

All notable changes to mado-news are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
