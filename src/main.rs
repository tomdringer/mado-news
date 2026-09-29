// mado-news — RSS/Atom news reader pixel plugin for Mado sidebar

use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(default)]
struct NewsConfig {
    feeds:        Vec<String>,
    refresh_secs: u64,
    max_per_feed: usize,
    /// Title font size in physical pixels. Default: 18.0.
    /// On a 2× retina display, 18 physical ≈ 9pt. Increase for larger text.
    font_size:    f32,
}

impl Default for NewsConfig {
    fn default() -> Self {
        Self {
            feeds: vec![
                "https://nerimasoft.co.uk/feed.xml".into(),
                "https://news.ycombinator.com/rss".into(),
                "https://www.theverge.com/rss/index.xml".into(),
            ],
            refresh_secs: 300,
            max_per_feed: 10,
            font_size:    18.0,
        }
    }
}

impl NewsConfig {
    fn load() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&home)
            .join(".config/mado/plugins/news.toml");
        let content = std::fs::read_to_string(path).unwrap_or_default();
        toml::from_str(&content).unwrap_or_default()
    }
}

// ── Palette (Slate) ───────────────────────────────────────────────────────────

const BG:      [u8; 4] = [15,  23,  42,  255]; // slate-900
const BG_ALT:  [u8; 4] = [22,  33,  55,  255]; // slate-850
const BG_SEL:  [u8; 4] = [37,  99,  235, 255]; // blue-600
const HEADER:  [u8; 4] = [100, 116, 139, 255]; // slate-500
const TEXT:    [u8; 4] = [226, 232, 240, 255]; // slate-200
const DIM:     [u8; 4] = [71,  85,  105, 255]; // slate-600
const ACCENT:  [u8; 4] = [56,  189, 248, 255]; // sky-400
const DIVIDER: [u8; 4] = [30,  41,  59,  255]; // slate-800

// ── Font loading ──────────────────────────────────────────────────────────────

static NERD_FONT_BYTES: &[u8] =
    include_bytes!("../assets/HackNerdFontMono-Regular.ttf");

fn load_nerd_font() -> fontdue::Font {
    fontdue::Font::from_bytes(NERD_FONT_BYTES, fontdue::FontSettings::default())
        .expect("mado-news: bundled Nerd Font load failed")
}

fn load_system_font() -> Option<fontdue::Font> {
    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = fontdue::Font::from_bytes(
                data.as_slice(), fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

// ── RSS / Atom feed parsing ───────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct NewsItem {
    title: String,
    url:   String,
    feed:  String,
}

fn parse_feed(xml: &str, source: &str, max: usize) -> Vec<NewsItem> {
    let mut items = Vec::new();
    let is_atom  = xml.contains("<feed") && xml.contains("xmlns");
    let item_tag = if is_atom { "<entry" } else { "<item" };
    let end_tag  = if is_atom { "</entry>" } else { "</item>" };

    let mut remaining = xml;
    while let Some(start) = remaining.find(item_tag) {
        let chunk_start = start + item_tag.len();
        let chunk_end = remaining[chunk_start..]
            .find(end_tag)
            .map(|e| chunk_start + e)
            .unwrap_or(remaining.len());
        let chunk = &remaining[chunk_start..chunk_end];

        let title = extract_tag(chunk, "title").unwrap_or_default();
        let title = strip_cdata(&title);
        let title = decode_html_entities(&title);

        let url = if is_atom {
            extract_attr(chunk, "link", "href")
                .or_else(|| extract_tag(chunk, "link"))
                .unwrap_or_default()
        } else {
            // Some feeds (e.g. Sky News) use <guid> as the permalink; fall back to it
            // when <link> is absent or not an http URL.
            let link = extract_tag(chunk, "link").unwrap_or_default();
            if link.trim_start().starts_with("http") {
                link
            } else {
                extract_tag(chunk, "guid")
                    .filter(|u| u.trim_start().starts_with("http"))
                    .unwrap_or(link)
            }
        };
        let url = url.trim().to_string();

        if !title.is_empty() && !url.is_empty()
            && (url.starts_with("http://") || url.starts_with("https://"))
        {
            items.push(NewsItem {
                title: title.trim().to_string(),
                url,
                feed: source.to_string(),
            });
        }

        remaining = &remaining[chunk_end..];
        if items.len() >= max { break; }
    }
    items
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open  = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let cs    = xml[start..].find('>')? + start + 1;
    let ce    = xml[cs..].find(&close)? + cs;
    Some(xml[cs..ce].to_string())
}

fn extract_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let open   = format!("<{}", tag);
    let needle = format!("{}=\"", attr);
    let ts = xml.find(&open)?;
    let te = xml[ts..].find('>')? + ts;
    let sl = &xml[ts..te];
    let vs = sl.find(&needle)? + needle.len();
    let ve = sl[vs..].find('"')? + vs;
    Some(sl[vs..ve].to_string())
}

fn strip_cdata(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("<![CDATA[") && s.ends_with("]]>") {
        s[9..s.len()-3].to_string()
    } else { s.to_string() }
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
     .replace("&lt;", "<")
     .replace("&gt;", ">")
     .replace("&quot;", "\"")
     .replace("&#39;", "'")
     .replace("&apos;", "'")
     .replace("&nbsp;", " ")
}

fn fetch_feed(url: &str, max: usize) -> Vec<NewsItem> {
    let source = url_to_source_name(url);
    let out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "15", "--location",
               "-A", "Mozilla/5.0 (compatible; mado-news/1.0)", url])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let xml = String::from_utf8_lossy(&o.stdout);
            parse_feed(&xml, &source, max)
        }
        _ => Vec::new(),
    }
}

fn url_to_source_name(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url);
    // Known overrides for common feeds
    let host = host.trim_start_matches("www.");
    match host {
        "news.ycombinator.com" => return "HN".into(),
        "feeds.bbci.co.uk"     => return "BBC".into(),
        "feeds.reuters.com"    => return "Reuters".into(),
        "feeds.arstechnica.com"=> return "Ars Technica".into(),
        "feeds.skynews.com"         => return "Sky News".into(),
        "news-api.cf.sky.com"       => return "Sky News".into(),
        _ => {}
    }
    // Generic: take the first meaningful domain component
    let part = host.split('.').next().unwrap_or(host);
    // Capitalise first letter only
    let mut chars = part.chars();
    match chars.next() {
        None    => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ── Canvas ────────────────────────────────────────────────────────────────────

struct Canvas { pixels: Vec<u8>, w: usize, h: usize }

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        let mut pixels = vec![0u8; w * h * 4];
        for px in pixels.chunks_exact_mut(4) { px.copy_from_slice(&BG); }
        Canvas { pixels, w, h }
    }

    fn fill_rect(&mut self, x: usize, y: usize, rw: usize, rh: usize, color: [u8; 4]) {
        for ry in y..y.saturating_add(rh).min(self.h) {
            for rx in x..x.saturating_add(rw).min(self.w) {
                let i = (ry * self.w + rx) * 4;
                self.pixels[i..i+4].copy_from_slice(&color);
            }
        }
    }

    fn hline(&mut self, x: usize, y: usize, len: usize, color: [u8; 4]) {
        if y >= self.h { return; }
        for rx in x..x.saturating_add(len).min(self.w) {
            let i = (y * self.w + rx) * 4;
            self.pixels[i..i+4].copy_from_slice(&color);
        }
    }

    fn blend(&mut self, x: usize, y: usize, color: [u8; 4], alpha: f32) {
        if x >= self.w || y >= self.h { return; }
        let i = (y * self.w + x) * 4;
        let ia = 1.0 - alpha;
        for c in 0..3 {
            self.pixels[i + c] =
                (self.pixels[i + c] as f32 * ia + color[c] as f32 * alpha).round() as u8;
        }
        self.pixels[i + 3] = 255;
    }

    fn text(&mut self, font: &fontdue::Font, text: &str,
            size: f32, x: usize, y: usize, color: [u8; 4]) -> usize {
        let mut cx = x;
        for ch in text.chars() {
            if font.lookup_glyph_index(ch) == 0 { continue; }
            let (m, bmp) = font.rasterize(ch, size);
            let gx = cx as isize + m.xmin as isize;
            let gy = y as isize - m.height as isize - m.ymin as isize;
            for (k, &cov) in bmp.iter().enumerate() {
                if cov == 0 { continue; }
                let px = gx + (k % m.width) as isize;
                let py = gy + (k / m.width) as isize;
                if px >= 0 && py >= 0 {
                    self.blend(px as usize, py as usize, color, cov as f32 / 255.0);
                }
            }
            cx += m.advance_width.round() as usize;
        }
        cx
    }

    fn measure(font: &fontdue::Font, text: &str, size: f32) -> usize {
        text.chars().map(|ch| {
            if font.lookup_glyph_index(ch) == 0 { return 0; }
            let (m, _) = font.rasterize(ch, size);
            m.advance_width.round() as usize
        }).sum()
    }

    /// Split `text` at the last word boundary that fits in `max_w`, returning
    /// (first line, remainder). Breaks mid-word if the first word alone is too wide.
    fn split_line<'a>(font: &fontdue::Font, text: &'a str,
                      size: f32, max_w: usize) -> (&'a str, &'a str) {
        let mut used     = 0usize;
        let mut last_fit = 0usize;
        let mut last_sp  = None;
        for (i, ch) in text.char_indices() {
            if ch == ' ' { last_sp = Some(i); }
            let (m, _) = font.rasterize(ch, size);
            used += m.advance_width.round() as usize;
            if used > max_w {
                let cut = last_sp.filter(|&sp| sp > 0).unwrap_or(last_fit);
                return (text[..cut].trim_end(), text[cut..].trim_start());
            }
            last_fit = i + ch.len_utf8();
        }
        (text, "")
    }

    fn text_clipped(&mut self, font: &fontdue::Font, text: &str,
                    size: f32, x: usize, y: usize, color: [u8; 4],
                    max_w: usize) -> usize {
        let full_w = Self::measure(font, text, size);
        if full_w <= max_w {
            return self.text(font, text, size, x, y, color);
        }
        let ew = Self::measure(font, "...", size);
        let avail = max_w.saturating_sub(ew);
        let mut used = 0usize;
        let mut end  = 0usize;
        for (i, ch) in text.char_indices() {
            if font.lookup_glyph_index(ch) == 0 { end = i + ch.len_utf8(); continue; }
            let (m, _) = font.rasterize(ch, size);
            let adv = m.advance_width.round() as usize;
            if used + adv > avail { break; }
            used += adv;
            end   = i + ch.len_utf8();
        }
        let t = format!("{}...", &text[..end]);
        self.text(font, &t, size, x, y, color)
    }

    fn write_frame(&self, out: &mut impl Write) {
        out.write_all(b"MADO").unwrap();
        out.write_all(&(self.w as u32).to_le_bytes()).unwrap();
        out.write_all(&(self.h as u32).to_le_bytes()).unwrap();
        out.write_all(&self.pixels).unwrap();
        out.flush().unwrap();
    }
}

// ── MACT ─────────────────────────────────────────────────────────────────────

fn send_browser_back(out: &mut impl Write) {
    let json  = br#"{"action":"browser_back"}"#;
    out.write_all(b"MACT").unwrap();
    out.write_all(&(json.len() as u32).to_le_bytes()).unwrap();
    out.write_all(json).unwrap();
    out.flush().unwrap();
}

fn send_navigate(out: &mut impl Write, url: &str) {
    let json = format!(r#"{{"action":"navigate","url":{}}}"#,
        serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into()));
    let bytes = json.as_bytes();
    out.write_all(b"MACT").unwrap();
    out.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
    out.write_all(bytes).unwrap();
    out.flush().unwrap();
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind:   String,
    width:  Option<u32>,
    height: Option<u32>,
    x:      Option<f32>,
    y:      Option<f32>,
    delta:  Option<f32>,
    text:   Option<String>,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct State {
    w:         u32,
    h:         u32,
    items:     Vec<NewsItem>,
    scroll:    usize,
    selected:  usize,
    loading:   bool,
    syncing:   bool,
    dirty:     bool,
    navigate:      Option<String>,
    browser_back:  bool,
    font_size: f32,
}

impl State {
    fn new(font_size: f32) -> Self {
        Self {
            w: 300, h: 600,
            items:     Vec::new(),
            scroll:    0,
            selected:  0,
            loading:   true,
            syncing:   false,
            dirty:     true,
            navigate:     None,
            browser_back: false,
            font_size,
        }
    }

    fn scroll_to_selected(&mut self) {
        let item_h      = item_height(self.font_size);
        let header_h    = (self.font_size * 2.2) as usize;
        let visible_h   = (self.h as usize).saturating_sub(header_h);
        let max_visible = if item_h > 0 { visible_h / item_h } else { 1 };
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if max_visible > 0 && self.selected >= self.scroll + max_visible {
            self.scroll = self.selected + 1 - max_visible;
        }
    }
}

// ── Layout ────────────────────────────────────────────────────────────────────
/// Row height: source label plus two title lines.
fn item_height(fs: f32) -> usize { (fs * 4.6) as usize }

// Derived from font_size at render time — see render() for how they're computed.
const PAD_X: usize = 14;

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render(state: &State, font: &fontdue::Font, nerd: &fontdue::Font, out: &mut impl Write) {
    let (w, h)      = (state.w as usize, state.h as usize);
    let fs          = state.font_size;
    let title_size  = fs;
    let label_size  = (fs * 0.82).max(10.0);
    let item_h      = item_height(fs);
    let header_h    = (fs * 2.2) as usize;

    let mut c = Canvas::new(w, h);

    // Header
    c.fill_rect(0, 0, w, header_h, BG_ALT);
    let icon_y = header_h - (fs * 0.5) as usize;
    c.text(nerd, "\u{F09E}", fs, PAD_X, icon_y, ACCENT);
    c.text(font, "NEWS", label_size, PAD_X + (fs * 1.3) as usize, icon_y, HEADER);
    // Sync icon (top-right of header) — press r to refresh
    let sync_icon  = if state.syncing { "\u{F254}" } else { "\u{F021}" }; // spinner vs refresh
    let sync_color = if state.syncing { DIM } else { HEADER };
    let sync_w     = Canvas::measure(nerd, sync_icon, fs);
    let sync_x     = w.saturating_sub(PAD_X + sync_w);
    c.text(nerd, sync_icon, fs, sync_x, icon_y, sync_color);
    // "r" hint to the left of the icon
    if !state.syncing {
        let hint_w = Canvas::measure(font, "r", label_size);
        let hint_x = sync_x.saturating_sub(hint_w + PAD_X / 2);
        c.text(font, "r", label_size, hint_x, icon_y, DIM);
    }
    c.hline(0, header_h - 1, w, DIVIDER);

    if state.loading && state.items.is_empty() {
        let msg = "Fetching feeds...";
        let mx  = (w.saturating_sub(Canvas::measure(font, msg, label_size))) / 2;
        c.text(font, msg, label_size, mx, h / 2, DIM);
        c.write_frame(out);
        return;
    }

    if state.items.is_empty() {
        let msg = "No items found";
        let mx  = (w.saturating_sub(Canvas::measure(font, msg, label_size))) / 2;
        c.text(font, msg, label_size, mx, h / 2, DIM);
        c.write_frame(out);
        return;
    }

    let visible_h   = h.saturating_sub(header_h);
    let max_visible = if item_h > 0 { visible_h / item_h } else { 1 };
    let max_scroll  = state.items.len().saturating_sub(max_visible.max(1));
    let scroll      = state.scroll.min(max_scroll);
    let end         = (scroll + max_visible + 1).min(state.items.len());

    for (vi, item) in state.items[scroll..end].iter().enumerate() {
        let item_idx    = scroll + vi;
        let iy          = header_h + vi * item_h;
        let is_selected = item_idx == state.selected;

        if is_selected {
            c.fill_rect(0, iy, w, item_h, BG_SEL);
        } else if vi % 2 == 1 {
            c.fill_rect(0, iy, w, item_h, BG_ALT);
        }

        // Source label (top of row)
        let label_y      = iy + (fs * 0.9) as usize;
        let source_color = if is_selected { TEXT } else { ACCENT };
        c.text(font, &item.feed, label_size, PAD_X, label_y, source_color);

        // Title — up to two lines, ellipsised on the second
        let title_y     = iy + (fs * 2.3) as usize;
        let max_title_w = w.saturating_sub(PAD_X * 2);
        let (line1, line2) = Canvas::split_line(font, &item.title, title_size, max_title_w);
        c.text(font, line1, title_size, PAD_X, title_y, TEXT);
        if !line2.is_empty() {
            c.text_clipped(font, line2, title_size,
                           PAD_X, title_y + (fs * 1.3) as usize, TEXT, max_title_w);
        }

        c.hline(PAD_X, iy + item_h - 1, w - PAD_X * 2, DIVIDER);
    }

    // Scrollbar
    if state.items.len() > max_visible && visible_h > 0 {
        let total   = state.items.len();
        let thumb_h = ((max_visible as f32 / total as f32) * visible_h as f32).max(20.0) as usize;
        let thumb_y = header_h + ((scroll as f32 / total as f32) * visible_h as f32) as usize;
        c.fill_rect(w - 3, thumb_y, 3, thumb_h, DIM);
    }

    c.write_frame(out);
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let cfg          = NewsConfig::load();
    let refresh_secs = cfg.refresh_secs;
    let max_per_feed = cfg.max_per_feed;
    let feeds        = cfg.feeds;

    let nerd_font = load_nerd_font();
    let sys_font  = load_system_font().unwrap_or_else(|| {
        eprintln!("mado-news: no system font found");
        std::process::exit(1);
    });

    let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State::new(cfg.font_size)));
    let force_refresh: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    // Feed fetcher thread
    {
        let state         = Arc::clone(&state);
        let force_refresh = Arc::clone(&force_refresh);
        let feeds         = feeds.clone();
        std::thread::spawn(move || loop {
            let mut all_items = Vec::new();
            for url in &feeds {
                all_items.extend(fetch_feed(url, max_per_feed));
            }
            {
                let mut s = state.lock().unwrap();
                s.items   = all_items;
                s.loading = false;
                s.syncing = false;
                s.dirty   = true;
            }
            // Sleep in 1-second chunks so a manual refresh wakes us early
            force_refresh.store(false, Ordering::Relaxed);
            for _ in 0..refresh_secs {
                std::thread::sleep(Duration::from_secs(1));
                if force_refresh.load(Ordering::Relaxed) { break; }
            }
        });
    }

    // Stdin event thread
    {
        let state         = Arc::clone(&state);
        let force_refresh = Arc::clone(&force_refresh);
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in BufReader::new(stdin.lock()).lines().flatten() {
                let ev: Event = match serde_json::from_str(&line) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let mut do_force_refresh = false;
                {
                let mut s = state.lock().unwrap();
                match ev.kind.as_str() {
                    "resize" => {
                        if let (Some(w), Some(h)) = (ev.width, ev.height) {
                            if w > 0 && h > 0 { s.w = w; s.h = h; s.dirty = true; }
                        }
                    }
                    "scroll" => {
                        if let Some(delta) = ev.delta {
                            let step = 3usize;
                            if delta > 0.0 {
                                s.scroll = s.scroll.saturating_add(step);
                            } else {
                                s.scroll = s.scroll.saturating_sub(step);
                            }
                            s.dirty = true;
                        }
                    }
                    "click" => {
                        if let Some(y) = ev.y {
                            let item_h   = (s.font_size * 3.5) as usize;
                            let header_h = (s.font_size * 2.2) as usize;
                            let iy = y as usize;
                            if iy < header_h {
                                // Click in header — right side is the sync button zone
                                let w = s.w as usize;
                                let sync_zone_x = w.saturating_sub(PAD_X * 3 + (s.font_size * 1.5) as usize);
                                if let Some(x) = ev.x {
                                    if x as usize >= sync_zone_x && !s.syncing {
                                        s.syncing      = true;
                                        s.dirty        = true;
                                        do_force_refresh = true;
                                    }
                                }
                            } else if item_h > 0 {
                                let vi  = (iy - header_h) / item_h;
                                let idx = s.scroll + vi;
                                if idx < s.items.len() {
                                    s.selected = idx;
                                    s.navigate = Some(s.items[idx].url.clone());
                                    s.dirty    = true;
                                }
                            }
                        }
                    }
                    "key" => {
                        if let Some(ref text) = ev.text.clone() {
                            // Mado sends Slint key text: arrows are U+F700–F703 and
                            // Return is "\n". The names are kept for older hosts.
                            match text.as_str() {
                                "ArrowDown" | "\u{F701}" | "j" => {
                                    if s.selected + 1 < s.items.len() {
                                        s.selected += 1;
                                        s.scroll_to_selected();
                                        s.dirty = true;
                                    }
                                }
                                "ArrowUp" | "\u{F700}" | "k" => {
                                    if s.selected > 0 {
                                        s.selected -= 1;
                                        s.scroll_to_selected();
                                        s.dirty = true;
                                    }
                                }
                                "Return" | "\n" | "\r" | "ArrowRight" | "\u{F703}" => {
                                    if let Some(item) = s.items.get(s.selected) {
                                        s.navigate = Some(item.url.clone());
                                        s.dirty    = true;
                                    }
                                }
                                "ArrowLeft" | "\u{F702}" => {
                                    s.browser_back = true;
                                }
                                "r" => {
                                    if !s.syncing {
                                        s.syncing = true;
                                        s.dirty   = true;
                                        do_force_refresh = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                } // drop state lock
                if do_force_refresh {
                    force_refresh.store(true, Ordering::Relaxed);
                }
            }
        });
    }

    // Render loop
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());

    loop {
        let (should_render, nav_url, go_back) = {
            let mut s  = state.lock().unwrap();
            let dirty  = s.dirty;
            s.dirty    = false;
            let nav    = s.navigate.take();
            let back   = s.browser_back;
            s.browser_back = false;
            (dirty, nav, back)
        };

        if let Some(url) = nav_url {
            send_navigate(&mut out, &url);
        }
        if go_back {
            send_browser_back(&mut out);
        }

        if should_render {
            let s = state.lock().unwrap();
            render(&s, &sys_font, &nerd_font, &mut out);
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}
