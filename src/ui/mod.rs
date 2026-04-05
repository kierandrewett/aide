use std::time::{SystemTime, UNIX_EPOCH};

use ansi_to_tui::IntoText;
use unicode_width::UnicodeWidthStr;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, FocusPanel};

const FOCUSED_BORDER: Color = Color::Cyan;

// ---------------------------------------------------------------------------
// Nerd Font icon helpers
// All code points are from the Nerd Fonts v2/v3 BMP range and work in any
// terminal configured with a Nerd Font (Ghostty, Termius desktop, etc.).
// ---------------------------------------------------------------------------

/// Returns a Nerd Font icon + trailing space for a file-browser entry.
/// Falls back to a plain two-space indent when icons are disabled.
fn nf_entry_icon(name: &str, is_dir: bool, expanded: bool) -> &'static str {
    if is_dir {
        return if expanded { "\u{f07c} " } else { "\u{f07b} " }; // fa-folder-open / fa-folder
    }
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    // Special filenames first
    match lower.as_str() {
        "dockerfile" | "containerfile" => return "\u{f308} ",
        "docker-compose.yml" | "docker-compose.yaml" => return "\u{f308} ",
        "makefile" | "gnumakefile" | "bsdmakefile" => return "\u{e779} ",
        "cmakelists.txt" => return "\u{e779} ",
        "cargo.toml" | "cargo.lock" => return "\u{e7a8} ",
        "package.json" | "package-lock.json" => return "\u{e74e} ",
        "tsconfig.json" | "jsconfig.json" => return "\u{e628} ",
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => return "\u{e702} ",
        ".editorconfig" => return "\u{e615} ",
        "license" | "licence" | "license.md" | "licence.md" | "license.txt" => {
            return "\u{f02d} "
        }
        "readme" | "readme.md" | "readme.txt" | "readme.rst" => return "\u{f48a} ",
        "jenkinsfile" => return "\u{e767} ",
        "vagrantfile" | "gemfile" | "rakefile" | "podfile" => return "\u{e21e} ",
        "brewfile" => return "\u{f0fc} ",
        _ => {}
    }

    // Extension mapping
    match ext {
        "rs" | "rlib" => "\u{e7a8} ",
        "py" | "pyw" | "pyi" | "pyc" => "\u{e73c} ",
        "js" | "mjs" | "cjs" => "\u{e74e} ",
        "ts" | "cts" | "mts" => "\u{e628} ",
        "jsx" => "\u{e7ba} ",
        "tsx" => "\u{e7ba} ",
        "html" | "htm" | "xhtml" => "\u{f13b} ",
        "css" => "\u{e749} ",
        "scss" | "sass" => "\u{e603} ",
        "less" => "\u{e758} ",
        "go" => "\u{e627} ",
        "rb" | "erb" | "gemspec" => "\u{e21e} ",
        "java" => "\u{e738} ",
        "kt" | "kts" => "\u{e70e} ",
        "scala" | "sc" => "\u{e737} ",
        "groovy" | "gvy" | "gradle" => "\u{e775} ",
        "clj" | "cljs" | "cljc" | "edn" => "\u{e76a} ",
        "c" => "\u{e61e} ",
        "h" => "\u{e61e} ",
        "cpp" | "cxx" | "cc" | "c++" => "\u{e61d} ",
        "hpp" | "hxx" | "hh" => "\u{e61d} ",
        "cs" | "csx" => "\u{f031b} ",
        "m" | "mm" => "\u{e61c} ",
        "swift" => "\u{e755} ",
        "hs" | "lhs" => "\u{e777} ",
        "ex" | "exs" | "heex" => "\u{e62d} ",
        "erl" | "hrl" => "\u{e7b1} ",
        "elm" => "\u{e62c} ",
        "ml" | "mli" | "fs" | "fsi" | "fsx" => "\u{e67a} ",
        "zig" => "\u{e6a9} ",
        "dart" => "\u{e798} ",
        "lua" => "\u{e620} ",
        "nim" | "nims" => "\u{e677} ",
        "cr" => "\u{e782} ",
        "sh" | "bash" | "bats" | "zsh" | "fish" => "\u{f489} ",
        "ps1" | "psm1" | "psd1" => "\u{f489} ",
        "json" | "json5" | "jsonc" => "\u{e60b} ",
        "yaml" | "yml" => "\u{f15c} ",
        "toml" => "\u{e6b2} ",
        "xml" | "xaml" | "svg" => "\u{e619} ",
        "ini" | "cfg" | "conf" | "config" | "env" => "\u{f013} ",
        "lock" => "󰌾 ",
        "properties" | "props" => "\u{f013} ",
        "md" | "mdx" | "markdown" => "\u{f48a} ",
        "txt" | "text" => "\u{f15c} ",
        "rst" | "rest" | "adoc" | "asciidoc" => "\u{f15c} ",
        "pdf" => "\u{f1c1} ",
        "graphql" | "gql" => "\u{e662} ",
        "prisma" => "\u{e662} ",
        "tf" | "tfvars" | "hcl" => "\u{e683} ",
        "nix" => "\u{f313} ",
        "ipynb" => "\u{e678} ",
        "r" | "rmd" => "\u{f25d} ",
        "jl" => "\u{e624} ",
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff" | "heic" => {
            "\u{f1c5} "
        }
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "flv" | "wmv" => "\u{f1c8} ",
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" => "\u{f1c7} ",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "7z" | "rar" | "tgz" => "\u{f1c6} ",
        "doc" | "docx" => "\u{f1c2} ",
        "xls" | "xlsx" => "\u{f1c3} ",
        "ppt" | "pptx" => "\u{f1c4} ",
        "diff" | "patch" => "\u{f440} ",
        "proto" => "\u{f0e8} ",
        _ => "\u{f15b} ",
    }
}

/// Folder colors keyed on the folder's lowercase name.
/// Colors sourced from the material-extensions/vscode-material-icon-theme SVG fills.
fn folder_color(lower: &str) -> Color {
    match lower {
        // ── Dot / hidden tool folders ─────────────────────────────────────
        ".agents"                        => Color::Rgb(255,  82,  82), // folder-robot
        ".claude"                        => Color::Rgb(255, 112,  67), // folder-claude
        ".cursor"                        => Color::Rgb(224, 224, 224), // folder-cursor
        ".gemini"                        => Color::Rgb( 66, 165, 245), // folder-gemini-ai
        ".github"                        => Color::Rgb( 84, 110, 122), // folder-github
        ".vscode"                        => Color::Rgb( 66, 165, 245), // folder-vscode
        ".idea"                          => Color::Rgb( 84, 110, 122), // folder-intellij
        ".fleet"                         => Color::Rgb( 99, 179, 237), // Fleet (no material icon)
        ".zed"                           => Color::Rgb( 84, 174, 255), // Zed (no material icon)
        ".cargo"                         => Color::Rgb(255, 112,  67), // folder-rust
        ".git"                           => Color::Rgb(255, 112,  67), // folder-git
        ".husky"                         => Color::Rgb( 96, 125, 139), // folder-husky
        ".next"                          => Color::Rgb( 84, 110, 122), // folder-next
        ".nuxt"                          => Color::Rgb( 84, 110, 122), // folder-nuxt
        ".svelte-kit" | ".sveltekit"     => Color::Rgb(255,  87,  34), // folder-svelte
        ".turbo"                         => Color::Rgb( 84, 110, 122), // folder-turborepo
        ".nx"                            => Color::Rgb( 22, 101, 216), // Nx (no material icon)
        ".yarn"                          => Color::Rgb(  2, 136, 209), // folder-yarn
        ".pnpm"                          => Color::Rgb(245, 165,  36), // pnpm (no material icon)
        ".npm"                           => Color::Rgb(203,  56,  55), // npm (no material icon)
        ".docker"                        => Color::Rgb(  3, 155, 229), // folder-docker
        ".terraform"                     => Color::Rgb( 92, 107, 192), // folder-terraform
        ".gradle"                        => Color::Rgb(  0, 151, 167), // folder-gradle
        ".mvn"                           => Color::Rgb(198,  40,  40), // Maven (no material icon)
        ".pytest_cache" | "__pycache__" | ".mypy_cache" | ".tox" | ".ruff_cache"
                                         => Color::Rgb( 66, 165, 245), // folder-python
        "venv" | ".venv" | "virtualenv" | ".virtualenv" | ".venv311" | ".venv312"
                                         => Color::Rgb(102, 187, 106), // folder-environment
        "node_modules"                   => Color::Rgb(139, 195,  74), // folder-node
        ".env" | ".envs"                 => Color::Rgb(102, 187, 106), // folder-environment

        // ── folder-rust ───────────────────────────────────────────────────
        "rust"                           => Color::Rgb(255, 112,  67),
        // ── folder-robot ──────────────────────────────────────────────────
        "bot" | "bots" | "robot" | "robots" | "agent" | "agents"
                                         => Color::Rgb(255,  82,  82),
        // ── folder-src ────────────────────────────────────────────────────
        "src" | "srcs" | "source" | "sources" | "code"
                                         => Color::Rgb( 76, 175,  80),
        // ── folder-dist ───────────────────────────────────────────────────
        "dist" | "out" | "output" | "outputs" | "build" | "builds"
        | "release" | "bin" | "distribution" | "built" | "compiled"
                                         => Color::Rgb(229, 115, 115),
        // ── folder-css ────────────────────────────────────────────────────
        "css" | "stylesheet" | "stylesheets" | "style" | "styles"
                                         => Color::Rgb(209, 196, 233),
        // ── folder-sass ───────────────────────────────────────────────────
        "sass" | "scss"                  => Color::Rgb(240,  98, 146),
        // ── folder-television ─────────────────────────────────────────────
        "tv" | "television"              => Color::Rgb(251, 192,  45),
        // ── folder-desktop ────────────────────────────────────────────────
        "desktop" | "display"            => Color::Rgb(  3, 155, 229),
        // ── folder-console ────────────────────────────────────────────────
        "console" | "xbox" | "ps4" | "ps5" | "switch" | "game" | "games"
                                         => Color::Rgb(139, 195,  74),
        // ── folder-images ─────────────────────────────────────────────────
        "images" | "image" | "imgs" | "img" | "icons" | "icon" | "icos" | "ico"
        | "figures" | "figure" | "figs" | "fig" | "screenshot" | "screenshots"
        | "screengrab" | "screengrabs" | "pic" | "pics" | "picture" | "pictures"
        | "photo" | "photos" | "photograph" | "photographs" | "texture" | "textures"
                                         => Color::Rgb(  0, 150, 136),
        // ── folder-scripts ────────────────────────────────────────────────
        "script" | "scripts" | "scripting"
                                         => Color::Rgb( 84, 110, 122),
        // ── folder-node ───────────────────────────────────────────────────
        "node" | "nodejs"                => Color::Rgb(139, 195,  74),
        // ── folder-javascript ─────────────────────────────────────────────
        "js" | "javascript" | "javascripts" | "cjs" | "mjs"
                                         => Color::Rgb(255, 202,  40),
        // ── folder-json ───────────────────────────────────────────────────
        "json" | "jsons" | "jsonc" | "jsonl"
                                         => Color::Rgb(249, 168,  37),
        // ── folder-font ───────────────────────────────────────────────────
        "font" | "fonts" | "typeface" | "typefaces"
                                         => Color::Rgb(255,  82,  82),
        // ── folder-bower ──────────────────────────────────────────────────
        "bower_components"               => Color::Rgb(141, 110,  99),
        // ── folder-test ───────────────────────────────────────────────────
        "test" | "tests" | "testing" | "snapshots" | "spec" | "specs"
        | "testfiles" | "__tests__"      => Color::Rgb(  0, 191, 165),
        // ── folder-directive ──────────────────────────────────────────────
        "directive" | "directives"       => Color::Rgb(244,  67,  54),
        // ── folder-jinja ──────────────────────────────────────────────────
        "jinja" | "jinja2" | "j2"        => Color::Rgb(224, 224, 224),
        // ── folder-markdown ───────────────────────────────────────────────
        "markdown" | "md"                => Color::Rgb(  2, 119, 189),
        // ── folder-pdm ────────────────────────────────────────────────────
        "pdm-plugins" | "pdm-build"      => Color::Rgb(149, 117, 205),
        // ── folder-php ────────────────────────────────────────────────────
        "php"                            => Color::Rgb( 30, 136, 229),
        // ── folder-phpmailer ──────────────────────────────────────────────
        "phpmailer"                      => Color::Rgb( 97,  97,  97),
        // ── folder-sublime ────────────────────────────────────────────────
        "sublime"                        => Color::Rgb( 97,  97,  97),
        // ── folder-docs ───────────────────────────────────────────────────
        "doc" | "docs" | "document" | "documents" | "documentation"
        | "post" | "posts" | "article" | "articles" | "wiki" | "news"
        | "blog" | "knowledge" | "diary" | "note" | "notes"
                                         => Color::Rgb(  2, 119, 189),
        // ── folder-gh-workflows ───────────────────────────────────────────
        "github/workflows"               => Color::Rgb( 84, 110, 122),
        // ── folder-git ────────────────────────────────────────────────────
        "git" | "patches" | "githooks" | "submodules"
                                         => Color::Rgb(255, 112,  67),
        // ── folder-github ─────────────────────────────────────────────────
        "github"                         => Color::Rgb( 84, 110, 122),
        // ── folder-gitea ──────────────────────────────────────────────────
        "gitea"                          => Color::Rgb(104, 159,  56),
        // ── folder-gitlab ─────────────────────────────────────────────────
        "gitlab"                         => Color::Rgb(117, 117, 117),
        // ── folder-forgejo ────────────────────────────────────────────────
        "forgejo"                        => Color::Rgb(117, 117, 117),
        // ── folder-vscode ─────────────────────────────────────────────────
        "vscode" | "vscode-test"         => Color::Rgb( 66, 165, 245),
        // ── folder-views ──────────────────────────────────────────────────
        "view" | "views" | "screen" | "screens" | "page" | "pages"
        | "public_html" | "html"         => Color::Rgb(255, 112,  67),
        // ── folder-vue ────────────────────────────────────────────────────
        "vue"                            => Color::Rgb(  0, 150, 136),
        // ── folder-vuepress ───────────────────────────────────────────────
        "vuepress"                       => Color::Rgb( 65, 184, 131),
        // ── folder-expo ───────────────────────────────────────────────────
        "expo" | "expo-shared"           => Color::Rgb( 25, 118, 210),
        // ── folder-config ─────────────────────────────────────────────────
        "cfg" | "cfgs" | "conf" | "confs" | "config" | "configs"
        | "configuration" | "configurations" | "setting" | "settings"
        | "meta-inf" | "option" | "options" | "pref" | "prefs"
        | "preference" | "preferences" | "props" | "properties"
                                         => Color::Rgb(  0, 172, 193),
        // ── folder-i18n ───────────────────────────────────────────────────
        "i18n" | "internationalization" | "lang" | "langs" | "language"
        | "languages" | "locale" | "locales" | "l10n" | "localization"
        | "translation" | "translate" | "translations" | "tx"
                                         => Color::Rgb( 92, 107, 192),
        // ── folder-components ─────────────────────────────────────────────
        "components" | "widget" | "widgets" | "fragments"
                                         => Color::Rgb(192, 202,  51),
        // ── folder-verdaccio ──────────────────────────────────────────────
        "verdaccio"                      => Color::Rgb(  0, 137, 123),
        // ── folder-aurelia ────────────────────────────────────────────────
        "aurelia_project"                => Color::Rgb(240,  98, 146),
        // ── folder-resource ───────────────────────────────────────────────
        "resource" | "resources" | "res" | "asset" | "assets"
        | "static" | "report" | "reports"
                                         => Color::Rgb(251, 192,  45),
        // ── folder-lib ────────────────────────────────────────────────────
        "lib" | "libs" | "library" | "libraries" | "vendor" | "vendors"
        | "third-party" | "lib64"        => Color::Rgb(192, 202,  51),
        // ── folder-theme ──────────────────────────────────────────────────
        "themes" | "theme" | "color" | "colors" | "colour" | "colours"
        | "design" | "designs" | "palette" | "palettes"
                                         => Color::Rgb( 30, 136, 229),
        // ── folder-webpack ────────────────────────────────────────────────
        "webpack"                        => Color::Rgb(  3, 169, 244),
        // ── folder-global ─────────────────────────────────────────────────
        "global"                         => Color::Rgb( 92, 107, 192),
        // ── folder-public ─────────────────────────────────────────────────
        "public" | "www" | "wwwroot" | "web" | "website" | "websites"
        | "site" | "browser" | "browsers" | "proxy"
                                         => Color::Rgb(  3, 155, 229),
        // ── folder-include ────────────────────────────────────────────────
        "inc" | "include" | "includes" | "partial" | "partials" | "inc64"
                                         => Color::Rgb(  3, 155, 229),
        // ── folder-docker ─────────────────────────────────────────────────
        "docker" | "dockerfiles" | "dockerhub"
                                         => Color::Rgb(  3, 155, 229),
        // ── folder-nginx ──────────────────────────────────────────────────
        "nginx"                          => Color::Rgb( 56, 142,  60),
        // ── folder-database ───────────────────────────────────────────────
        "db" | "data" | "database" | "databases" | "sql"
                                         => Color::Rgb(255, 202,  40),
        // ── folder-migrations ─────────────────────────────────────────────
        "migrations" | "migration"       => Color::Rgb(236,  64, 122),
        // ── folder-log ────────────────────────────────────────────────────
        "log" | "logs" | "logging"       => Color::Rgb(175, 180,  43),
        // ── folder-target ─────────────────────────────────────────────────
        "target"                         => Color::Rgb( 84, 110, 122),
        // ── folder-temp ───────────────────────────────────────────────────
        "temp" | "tmp" | "cached" | "cache" | "caches" | ".cache" | ".tmp"
                                         => Color::Rgb(  0, 151, 167),
        // ── folder-aws ────────────────────────────────────────────────────
        "aws" | "azure" | "gcp"          => Color::Rgb(255, 179,   0),
        // ── folder-audio ──────────────────────────────────────────────────
        "aud" | "auds" | "audio" | "audios" | "music" | "song" | "songs"
        | "sound" | "sounds" | "voice" | "voices" | "recordings"
        | "playlist" | "playlists"       => Color::Rgb(239,  83,  80),
        // ── folder-video ──────────────────────────────────────────────────
        "vid" | "vids" | "video" | "videos" | "movie" | "movies" | "media"
                                         => Color::Rgb(255, 152,   0),
        // ── folder-kubernetes ─────────────────────────────────────────────
        "kubernetes" | "k8s"             => Color::Rgb( 68, 138, 255),
        // ── folder-import ─────────────────────────────────────────────────
        "import" | "imports" | "imported"
                                         => Color::Rgb(175, 180,  43),
        // ── folder-export ─────────────────────────────────────────────────
        "export" | "exports" | "exported"
                                         => Color::Rgb(255, 138, 101),
        // ── folder-wakatime ───────────────────────────────────────────────
        "wakatime"                       => Color::Rgb( 69,  90, 100),
        // ── folder-circleci ───────────────────────────────────────────────
        "circleci"                       => Color::Rgb( 84, 110, 122),
        // ── folder-wordpress ──────────────────────────────────────────────
        "wordpress-org" | "wp-content"   => Color::Rgb(  2, 119, 189),
        // ── folder-gradle ─────────────────────────────────────────────────
        "gradle"                         => Color::Rgb(  0, 151, 167),
        // ── folder-coverage ───────────────────────────────────────────────
        "coverage" | ".coverage" | ".nyc_output" | "nyc-output" | "nyc_output"
        | "e2e" | "it" | "integration-test" | "integration-tests"
                                         => Color::Rgb( 38, 166, 154),
        // ── folder-class ──────────────────────────────────────────────────
        "class" | "classes" | "model" | "models" | "schemas" | "schema"
                                         => Color::Rgb(239,  83,  80),
        // ── folder-other ──────────────────────────────────────────────────
        "other" | "others" | "misc" | "miscellaneous" | "extra" | "extras" | "etc"
                                         => Color::Rgb(255, 112,  67),
        // ── folder-lua ────────────────────────────────────────────────────
        "lua"                            => Color::Rgb( 66, 165, 245),
        // ── folder-turborepo ──────────────────────────────────────────────
        "turbo"                          => Color::Rgb( 84, 110, 122),
        // ── folder-typescript ─────────────────────────────────────────────
        "typescript" | "ts" | "typings" | "@types" | "types" | "cts" | "mts"
                                         => Color::Rgb(  2, 136, 209),
        // ── folder-graphql ────────────────────────────────────────────────
        "graphql" | "gql"                => Color::Rgb(236,  64, 122),
        // ── folder-routes ─────────────────────────────────────────────────
        "routes" | "router" | "routers" | "navigation" | "navigations" | "routing"
                                         => Color::Rgb( 67, 160,  71),
        // ── folder-ci ─────────────────────────────────────────────────────
        "ci"                             => Color::Rgb(  2, 136, 209),
        // ── folder-eslint ─────────────────────────────────────────────────
        "eslint" | "eslint-plugin" | "eslint-plugins" | "eslint-config" | "eslint-configs"
                                         => Color::Rgb( 92, 107, 192),
        // ── folder-benchmark ──────────────────────────────────────────────
        "benchmark" | "benchmarks" | "bench" | "benches" | "performance" | "perf"
        | "profiling" | "measure" | "measures" | "measurement"
                                         => Color::Rgb( 30, 136, 229),
        // ── folder-messages ───────────────────────────────────────────────
        "messages" | "messaging" | "forum" | "chat" | "chats" | "conversation"
        | "conversations" | "dialog" | "dialogs"
                                         => Color::Rgb(255, 152,   0),
        // ── folder-less ───────────────────────────────────────────────────
        "less"                           => Color::Rgb(  2, 119, 189),
        // ── folder-gulp ───────────────────────────────────────────────────
        "gulp" | "gulp-tasks" | "gulpfiles"
                                         => Color::Rgb(255,  82,  82),
        // ── folder-python ─────────────────────────────────────────────────
        "python" | "pycache" | "pytest_cache"
                                         => Color::Rgb( 66, 165, 245),
        // ── folder-r ──────────────────────────────────────────────────────
        "r"                              => Color::Rgb( 25, 118, 210),
        // ── folder-sandbox ────────────────────────────────────────────────
        "sandbox" | "sandboxes" | "playground" | "playgrounds"
                                         => Color::Rgb( 30, 136, 229),
        // ── folder-scons ──────────────────────────────────────────────────
        "scons" | "sconf_temp" | "scons_cache"
                                         => Color::Rgb(239,  83,  80),
        // ── folder-mojo ───────────────────────────────────────────────────
        "mojo"                           => Color::Rgb(255, 112,  67),
        // ── folder-moon ───────────────────────────────────────────────────
        "moon"                           => Color::Rgb(126,  87, 194),
        // ── folder-debug ──────────────────────────────────────────────────
        "debug" | "debugger" | "debugging"
                                         => Color::Rgb(249, 168,  37),
        // ── folder-fastlane ───────────────────────────────────────────────
        "fastlane"                       => Color::Rgb( 30, 136, 229),
        // ── folder-plugin ─────────────────────────────────────────────────
        "plugin" | "plugins" | "mod" | "mods" | "modding" | "extension"
        | "extensions" | "addon" | "addons" | "addin" | "addins"
        | "module" | "modules"           => Color::Rgb(  2, 136, 209),
        // ── folder-middleware ─────────────────────────────────────────────
        "middleware" | "middlewares"     => Color::Rgb( 92, 107, 192),
        // ── folder-controller ─────────────────────────────────────────────
        "controller" | "controllers" | "controls" | "service" | "services"
        | "provider" | "providers" | "handler" | "handlers"
                                         => Color::Rgb(255, 193,   7),
        // ── folder-ansible ────────────────────────────────────────────────
        "ansible"                        => Color::Rgb( 97,  97,  97),
        // ── folder-server ─────────────────────────────────────────────────
        "server" | "servers" | "backend" | "backends" | "inventory"
        | "inventories" | "infrastructure" | "infra"
                                         => Color::Rgb(251, 192,  45),
        // ── folder-client ─────────────────────────────────────────────────
        "client" | "clients" | "frontend" | "frontends" | "pwa" | "spa"
                                         => Color::Rgb(  3, 155, 229),
        // ── folder-tasks ──────────────────────────────────────────────────
        "tasks" | "tickets"              => Color::Rgb( 92, 107, 192),
        // ── folder-android ────────────────────────────────────────────────
        "android"                        => Color::Rgb(139, 195,  74),
        // ── folder-ios ────────────────────────────────────────────────────
        "ios"                            => Color::Rgb(120, 144, 156),
        // ── folder-ui ─────────────────────────────────────────────────────
        "presentation" | "gui" | "ui" | "ux"
                                         => Color::Rgb(171,  71, 188),
        // ── folder-upload ─────────────────────────────────────────────────
        "uploads" | "upload"             => Color::Rgb(255, 112,  67),
        // ── folder-download ───────────────────────────────────────────────
        "downloads" | "download" | "downloader" | "downloaders"
                                         => Color::Rgb( 76, 175,  80),
        // ── folder-tools ──────────────────────────────────────────────────
        "tools" | "toolkit" | "toolkits" | "toolbox" | "toolboxes"
        | "tooling" | "devtools" | "kit" | "kits"
                                         => Color::Rgb( 30, 136, 229),
        // ── folder-helper ─────────────────────────────────────────────────
        "helpers" | "helper"             => Color::Rgb(175, 180,  43),
        // ── folder-serverless ─────────────────────────────────────────────
        "serverless"                     => Color::Rgb(239,  83,  80),
        // ── folder-api ────────────────────────────────────────────────────
        "api" | "apis" | "restapi"       => Color::Rgb(251, 192,  45),
        // ── folder-app ────────────────────────────────────────────────────
        "app" | "apps" | "application" | "applications"
                                         => Color::Rgb(239,  83,  80),
        // ── folder-apollo ─────────────────────────────────────────────────
        "apollo" | "apollo-client" | "apollo-cache" | "apollo-config"
                                         => Color::Rgb(126,  87, 194),
        // ── folder-archive ────────────────────────────────────────────────
        "arc" | "arcs" | "archive" | "archives" | "archival"
                                         => Color::Rgb(255, 167,  38),
        // ── folder-backup ─────────────────────────────────────────────────
        "bkp" | "bkps" | "bak" | "baks" | "backup" | "backups"
        | "back-up" | "back-ups" | "history" | "histories"
                                         => Color::Rgb(141, 110,  99),
        // ── folder-batch ──────────────────────────────────────────────────
        "batch" | "batchs" | "batches"   => Color::Rgb( 97,  97,  97),
        // ── folder-buildkite ──────────────────────────────────────────────
        "buildkite"                      => Color::Rgb( 76, 175,  80),
        // ── folder-cluster ────────────────────────────────────────────────
        "cluster" | "clusters"           => Color::Rgb( 38, 166, 154),
        // ── folder-command ────────────────────────────────────────────────
        "command" | "commands" | "commandline" | "cmd" | "cli" | "clis"
                                         => Color::Rgb(  3, 169, 244),
        // ── folder-constant ───────────────────────────────────────────────
        "constant" | "constants" | "const" | "consts"
                                         => Color::Rgb( 96, 125, 139),
        // ── folder-container ──────────────────────────────────────────────
        "container" | "containers" | "devcontainer"
                                         => Color::Rgb(  2, 136, 209),
        // ── folder-content ────────────────────────────────────────────────
        "content" | "contents"           => Color::Rgb(  0, 188, 212),
        // ── folder-context ────────────────────────────────────────────────
        "context" | "contexts"           => Color::Rgb(  0, 137, 123),
        // ── folder-core ───────────────────────────────────────────────────
        "core"                           => Color::Rgb( 25, 118, 210),
        // ── folder-delta ──────────────────────────────────────────────────
        "delta" | "deltas" | "changes"   => Color::Rgb(236,  64, 122),
        // ── folder-dump ───────────────────────────────────────────────────
        "dump" | "dumps"                 => Color::Rgb(117, 117, 117),
        // ── folder-examples ───────────────────────────────────────────────
        "demo" | "demos" | "example" | "examples" | "sample" | "samples" | "sample-data"
                                         => Color::Rgb(  0, 150, 136),
        // ── folder-environment ────────────────────────────────────────────
        "env" | "envs" | "environment" | "environments"
                                         => Color::Rgb(102, 187, 106),
        // ── folder-functions ──────────────────────────────────────────────
        "func" | "funcs" | "function" | "functions" | "lambda" | "lambdas"
        | "logic" | "math" | "maths" | "calc" | "calcs" | "calculation"
        | "calculations" | "composable" | "composables"
                                         => Color::Rgb(  2, 136, 209),
        // ── folder-generator ──────────────────────────────────────────────
        "generator" | "generators" | "generated" | "cfn-gen" | "gen" | "gens" | "auto"
        | "__generated__"                => Color::Rgb(239,  83,  80),
        // ── folder-hook ───────────────────────────────────────────────────
        "hook" | "hooks"                 => Color::Rgb(126,  87, 194),
        // ── folder-trigger ────────────────────────────────────────────────
        "trigger" | "triggers"           => Color::Rgb(255, 193,   7),
        // ── folder-job ────────────────────────────────────────────────────
        "job" | "jobs"                   => Color::Rgb(255, 160,   0),
        // ── folder-keys ───────────────────────────────────────────────────
        "key" | "keys" | "token" | "tokens" | "jwt" | "secret" | "secrets"
                                         => Color::Rgb( 38, 166, 154),
        // ── folder-layout ─────────────────────────────────────────────────
        "layout" | "layouts"             => Color::Rgb(  3, 155, 229),
        // ── folder-mail ───────────────────────────────────────────────────
        "mail" | "mails" | "email" | "emails" | "smtp" | "mailers"
                                         => Color::Rgb(255,  82,  82),
        // ── folder-mappings ───────────────────────────────────────────────
        "mappings" | "mapping"           => Color::Rgb(175, 180,  43),
        // ── folder-meta ───────────────────────────────────────────────────
        "meta" | "metadata"              => Color::Rgb(124, 179,  66),
        // ── folder-changesets ─────────────────────────────────────────────
        "changesets" | "changeset"       => Color::Rgb( 33, 150, 243),
        // ── folder-packages ───────────────────────────────────────────────
        "package" | "packages" | "pkg" | "pkgs" | "serverpackages"
        | "devpackages" | "dependencies" => Color::Rgb( 30, 136, 229),
        // ── folder-shared ─────────────────────────────────────────────────
        "shared" | "common"              => Color::Rgb(171,  71, 188),
        // ── folder-shader ─────────────────────────────────────────────────
        "glsl" | "hlsl" | "shader" | "shaders"
                                         => Color::Rgb(171,  71, 188),
        // ── folder-stack ──────────────────────────────────────────────────
        "stack" | "stacks"               => Color::Rgb(255, 167,  38),
        // ── folder-template ───────────────────────────────────────────────
        "template" | "templates"         => Color::Rgb(141, 110,  99),
        // ── folder-utils ──────────────────────────────────────────────────
        "util" | "utils" | "utility" | "utilities"
                                         => Color::Rgb(124, 179,  66),
        // ── folder-supabase ───────────────────────────────────────────────
        "supabase"                       => Color::Rgb(102, 187, 106),
        // ── folder-private ────────────────────────────────────────────────
        "private"                        => Color::Rgb(255,  82,  82),
        // ── folder-linux ──────────────────────────────────────────────────
        "linux" | "linuxbsd" | "unix" | "wsl" | "ubuntu" | "deb" | "debian"
        | "deepin" | "centos" | "popos" | "mint"
                                         => Color::Rgb(249, 168,  37),
        // ── folder-windows ────────────────────────────────────────────────
        "windows" | "win" | "win32" | "windows11" | "windows10" | "windowsxp"
        | "windowsnt" | "win11" | "win10" | "winxp" | "winnt"
                                         => Color::Rgb( 33, 150, 243),
        // ── folder-macos ──────────────────────────────────────────────────
        "macos" | "mac" | "osx" | "ds_store" | "iphone" | "ipad" | "ipod"
        | "macbook" | "macbook-air" | "macosx" | "apple"
                                         => Color::Rgb( 84, 110, 122),
        // ── folder-error ──────────────────────────────────────────────────
        "error" | "errors" | "err" | "errs" | "crash" | "crashes"
                                         => Color::Rgb(239,  83,  80),
        // ── folder-event ──────────────────────────────────────────────────
        "event" | "events"               => Color::Rgb(251, 192,  45),
        // ── folder-secure ─────────────────────────────────────────────────
        "auth" | "authentication" | "secure" | "security" | "cert" | "certs"
        | "certificate" | "certificates" | "ssl" | "cipher" | "cypher" | "tls"
                                         => Color::Rgb(249, 168,  37),
        // ── folder-custom ─────────────────────────────────────────────────
        "custom" | "customs"             => Color::Rgb(255, 112,  67),
        // ── folder-mock ───────────────────────────────────────────────────
        "draft" | "drafts" | "mock" | "mocks" | "__mocks__" | "fixture" | "fixtures"
        | "concept" | "concepts" | "sketch" | "sketches" | "stub" | "stubs"
        | "fake" | "fakes"               => Color::Rgb(141, 110,  99),
        // ── folder-syntax ─────────────────────────────────────────────────
        "syntax" | "syntaxes" | "spellcheck" | "spellcheckers"
                                         => Color::Rgb(239,  83,  80),
        // ── folder-vm ─────────────────────────────────────────────────────
        "vm" | "vms"                     => Color::Rgb(  3, 155, 229),
        // ── folder-stylus ─────────────────────────────────────────────────
        "stylus"                         => Color::Rgb(192, 202,  51),
        // ── folder-flow ───────────────────────────────────────────────────
        "flow-typed"                     => Color::Rgb( 84, 110, 122),
        // ── folder-rules ──────────────────────────────────────────────────
        "rule" | "rules" | "validation" | "validations" | "validator" | "validators"
                                         => Color::Rgb(239,  83,  80),
        // ── folder-review ─────────────────────────────────────────────────
        "review" | "reviews" | "revisal" | "revisals" | "reviewed" | "preview" | "previews"
                                         => Color::Rgb( 33, 150, 243),
        // ── folder-animation ──────────────────────────────────────────────
        "anim" | "anims" | "animation" | "animations" | "animated" | "motion"
        | "motions" | "transition" | "transitions" | "easing" | "easings"
                                         => Color::Rgb(236,  64, 122),
        // ── folder-guard ──────────────────────────────────────────────────
        "guard" | "guards"               => Color::Rgb( 67, 160,  71),
        // ── folder-prisma ─────────────────────────────────────────────────
        "prisma"                         => Color::Rgb(  0, 191, 165),
        // ── folder-pipe ───────────────────────────────────────────────────
        "pipe" | "pipes" | "pipeline" | "pipelines"
                                         => Color::Rgb(  0, 137, 123),
        // ── folder-interceptor ────────────────────────────────────────────
        "interceptor" | "interceptors"   => Color::Rgb(255, 152,   0),
        // ── folder-svg ────────────────────────────────────────────────────
        "svg" | "svgs" | "vector" | "vectors"
                                         => Color::Rgb(255, 179,   0),
        // ── folder-nuxt (non-dot) ─────────────────────────────────────────
        "nuxt"                           => Color::Rgb( 84, 110, 122),
        // ── folder-terraform (non-dot) ────────────────────────────────────
        "terraform"                      => Color::Rgb( 92, 107, 192),
        // ── folder-mobile ─────────────────────────────────────────────────
        "mobile" | "mobiles" | "portable" | "portability" | "phone" | "phones"
                                         => Color::Rgb(255,  82,  82),
        // ── folder-stencil ────────────────────────────────────────────────
        "stencil"                        => Color::Rgb( 68, 138, 255),
        // ── folder-firebase ───────────────────────────────────────────────
        "firebase"                       => Color::Rgb(255, 145,   0),
        // ── folder-firestore ──────────────────────────────────────────────
        "firestore" | "cloud-firestore" | "firebase-firestore"
                                         => Color::Rgb( 33, 150, 243),
        // ── folder-cloud-functions ────────────────────────────────────────
        "cloud-functions" | "cloudfunctions" | "firebase-cloud-functions"
        | "firebase-cloudfunctions"      => Color::Rgb(187, 222, 251),
        // ── folder-svelte ─────────────────────────────────────────────────
        "svelte" | "svelte-kit"          => Color::Rgb(255,  87,  34),
        // ── folder-update ─────────────────────────────────────────────────
        "update" | "updates" | "upgrade" | "upgrades"
                                         => Color::Rgb( 67, 160,  71),
        // ── folder-mjml ───────────────────────────────────────────────────
        "mjml"                           => Color::Rgb(255,  87,  34),
        // ── folder-admin ──────────────────────────────────────────────────
        "admin" | "admins" | "manager" | "managers" | "moderator" | "moderators"
                                         => Color::Rgb( 84, 110, 122),
        // ── folder-jupyter ────────────────────────────────────────────────
        "jupyter" | "notebook" | "notebooks" | "ipynb"
                                         => Color::Rgb(255, 152,   0),
        // ── folder-scala ──────────────────────────────────────────────────
        "scala"                          => Color::Rgb(244,  67,  54),
        // ── folder-connection ─────────────────────────────────────────────
        "connection" | "connections" | "integration" | "integrations"
        | "remote" | "remotes"           => Color::Rgb(  0, 172, 193),
        // ── folder-quasar ─────────────────────────────────────────────────
        "quasar"                         => Color::Rgb( 25, 118, 210),
        // ── folder-next (non-dot) ─────────────────────────────────────────
        "next"                           => Color::Rgb( 84, 110, 122),
        // ── folder-dal ────────────────────────────────────────────────────
        "dal" | "data-access" | "data-access-layer"
                                         => Color::Rgb(244,  67,  54),
        // ── folder-cobol ──────────────────────────────────────────────────
        "cobol"                          => Color::Rgb(  2, 136, 209),
        // ── folder-yarn (non-dot) ─────────────────────────────────────────
        "yarn"                           => Color::Rgb(  2, 136, 209),
        // ── folder-husky (non-dot) ────────────────────────────────────────
        "husky"                          => Color::Rgb( 96, 125, 139),
        // ── folder-storybook ──────────────────────────────────────────────
        "storybook" | "stories" | "story"
                                         => Color::Rgb(255,  64, 129),
        // ── folder-base ───────────────────────────────────────────────────
        "base" | "bases"                 => Color::Rgb(141, 110,  99),
        // ── folder-cart ───────────────────────────────────────────────────
        "cart" | "shopping-cart" | "shopping" | "shop"
                                         => Color::Rgb(  0, 150, 136),
        // ── folder-home ───────────────────────────────────────────────────
        "home" | "start" | "main" | "landing"
                                         => Color::Rgb(255,  82,  82),
        // ── folder-project ────────────────────────────────────────────────
        "project" | "projects" | "proj" | "projs"
                                         => Color::Rgb( 30, 136, 229),
        // ── folder-prompts ────────────────────────────────────────────────
        "prompt" | "prompts"             => Color::Rgb( 92, 107, 192),
        // ── folder-interface ──────────────────────────────────────────────
        "interface" | "interfaces"       => Color::Rgb( 25, 118, 210),
        // ── folder-netlify ────────────────────────────────────────────────
        "netlify"                        => Color::Rgb( 38, 166, 154),
        // ── folder-enum ───────────────────────────────────────────────────
        "enum" | "enums"                 => Color::Rgb( 38, 166, 154),
        // ── folder-contract ───────────────────────────────────────────────
        "pact" | "pacts" | "contract" | "contracts" | "contract-testing"
        | "contract-test" | "contract-tests"
                                         => Color::Rgb( 68, 138, 255),
        // ── folder-helm ───────────────────────────────────────────────────
        "helm" | "helmchart" | "helmcharts"
                                         => Color::Rgb(  0, 172, 193),
        // ── folder-queue ──────────────────────────────────────────────────
        "queue" | "queues" | "bull" | "mq"
                                         => Color::Rgb(  3, 155, 229),
        // ── folder-vercel ─────────────────────────────────────────────────
        "vercel" | "now"                 => Color::Rgb( 84, 110, 122),
        // ── folder-cypress ────────────────────────────────────────────────
        "cypress"                        => Color::Rgb(  0, 150, 136),
        // ── folder-decorators ─────────────────────────────────────────────
        "decorator" | "decorators"       => Color::Rgb(171,  71, 188),
        // ── folder-java ───────────────────────────────────────────────────
        "java"                           => Color::Rgb(239,  83,  80),
        // ── folder-resolver ───────────────────────────────────────────────
        "resolver" | "resolvers"         => Color::Rgb( 67, 160,  71),
        // ── folder-angular ────────────────────────────────────────────────
        "angular"                        => Color::Rgb(255,  82,  82),
        // ── folder-unity ──────────────────────────────────────────────────
        "unity"                          => Color::Rgb( 33, 150, 243),
        // ── folder-pdf ────────────────────────────────────────────────────
        "pdf" | "pdfs"                   => Color::Rgb(239,  83,  80),
        // ── folder-proto ──────────────────────────────────────────────────
        "protobuf" | "protobufs" | "proto" | "protos"
                                         => Color::Rgb(255, 112,  67),
        // ── folder-plastic ────────────────────────────────────────────────
        "plastic"                        => Color::Rgb(255, 152,   0),
        // ── folder-gamemaker ──────────────────────────────────────────────
        "gamemaker" | "gamemaker2"       => Color::Rgb( 38, 166, 154),
        // ── folder-mercurial ──────────────────────────────────────────────
        "hg" | "hghooks" | "hgext"       => Color::Rgb(120, 144, 156),
        // ── folder-godot ──────────────────────────────────────────────────
        "godot" | "godot-cpp"            => Color::Rgb( 66, 165, 245),
        // ── folder-lottie ─────────────────────────────────────────────────
        "lottie" | "lotties" | "lottiefiles"
                                         => Color::Rgb(  0, 191, 165),
        // ── folder-taskfile ───────────────────────────────────────────────
        "taskfile" | "taskfiles"         => Color::Rgb( 77, 182, 172),
        // ── folder-drizzle ────────────────────────────────────────────────
        "drizzle"                        => Color::Rgb( 76, 175,  80),
        // ── folder-cloudflare ─────────────────────────────────────────────
        "cloudflare"                     => Color::Rgb(255, 138, 101),
        // ── folder-seeders ────────────────────────────────────────────────
        "seeds" | "seeders" | "seed" | "seeding"
                                         => Color::Rgb( 67, 160,  71),
        // ── folder-bicep ──────────────────────────────────────────────────
        "bicep"                          => Color::Rgb(251, 192,  45),
        // ── folder-snapcraft ──────────────────────────────────────────────
        "snap" | "snapcraft"             => Color::Rgb(102, 187, 106),
        // ── folder-development ────────────────────────────────────────────
        "dev" | "development"            => Color::Rgb(  2, 136, 209),
        // ── folder-flutter ────────────────────────────────────────────────
        "flutter"                        => Color::Rgb(  3, 169, 244),
        // ── folder-snippet ────────────────────────────────────────────────
        "snippet" | "snippets"           => Color::Rgb(255, 152,   0),
        // ── folder-element ────────────────────────────────────────────────
        "element" | "elements"           => Color::Rgb(171,  71, 188),
        // ── folder-src-tauri ──────────────────────────────────────────────
        "src-tauri"                      => Color::Rgb( 69,  90, 100),
        // ── folder-favicon ────────────────────────────────────────────────
        "favicon" | "favicons"           => Color::Rgb(251, 192,  45),
        // ── folder-features ───────────────────────────────────────────────
        "feature" | "features" | "feat" | "feats"
                                         => Color::Rgb(104, 159,  56),
        // ── folder-lefthook ───────────────────────────────────────────────
        "lefthook" | "lefthook-local"    => Color::Rgb( 96, 125, 139),
        // ── folder-bloc ───────────────────────────────────────────────────
        "bloc" | "cubit" | "blocs" | "cubits"
                                         => Color::Rgb( 38, 166, 154),
        // ── folder-powershell ─────────────────────────────────────────────
        "powershell" | "ps" | "ps1"      => Color::Rgb(  3, 169, 244),
        // ── folder-repository ─────────────────────────────────────────────
        "repository" | "repositories" | "repo" | "repos"
                                         => Color::Rgb( 67, 160,  71),
        // ── folder-luau ───────────────────────────────────────────────────
        "luau"                           => Color::Rgb( 66, 165, 245),
        // ── folder-obsidian ───────────────────────────────────────────────
        "obsidian"                       => Color::Rgb(103,  58, 183),
        // ── folder-trash ──────────────────────────────────────────────────
        "trash"                          => Color::Rgb(244,  67,  54),
        // ── folder-cline ──────────────────────────────────────────────────
        "cline_docs"                     => Color::Rgb( 66, 165, 245),
        // ── folder-liquibase ──────────────────────────────────────────────
        "liquibase"                      => Color::Rgb(244,  67,  54),
        // ── folder-dart ───────────────────────────────────────────────────
        "dart" | "dart_tool" | "dart_tools"
                                         => Color::Rgb( 33, 150, 243),
        // ── folder-zeabur ─────────────────────────────────────────────────
        "zeabur"                         => Color::Rgb(126,  87, 194),
        // ── folder-kusto ──────────────────────────────────────────────────
        "kusto" | "kql"                  => Color::Rgb( 30, 136, 229),
        // ── folder-policy ─────────────────────────────────────────────────
        "policy" | "policies"            => Color::Rgb(  2, 136, 209),
        // ── folder-attachment ─────────────────────────────────────────────
        "attachment" | "attachments"     => Color::Rgb(156,  39, 176),
        // ── folder-bibliography ───────────────────────────────────────────
        "bibliography" | "bibliographies" | "book" | "books"
                                         => Color::Rgb(161, 136, 127),
        // ── folder-link ───────────────────────────────────────────────────
        "link" | "links"                 => Color::Rgb(126,  87, 194),
        // ── folder-pytorch ────────────────────────────────────────────────
        "pytorch" | "torch"              => Color::Rgb(244,  81,  30),
        // ── folder-blender ────────────────────────────────────────────────
        "blender" | "blender-assets" | "blender-files" | "blender-project"
        | "blender-models"               => Color::Rgb(255, 152,   0),
        // ── folder-atom ───────────────────────────────────────────────────
        "atoms" | "atom"                 => Color::Rgb( 30, 136, 229),
        // ── folder-molecule ───────────────────────────────────────────────
        "molecules" | "molecule"         => Color::Rgb(255, 152,   0),
        // ── folder-organism ───────────────────────────────────────────────
        "organisms" | "organism"         => Color::Rgb(  0, 150, 136),
        // ── folder-claude (non-dot) ───────────────────────────────────────
        "claude"                         => Color::Rgb(255, 112,  67),
        // ── folder-gemini-ai (non-dot) ────────────────────────────────────
        "gemini" | "gemini-ai" | "geminiai"
                                         => Color::Rgb( 66, 165, 245),
        // ── folder-input ──────────────────────────────────────────────────
        "input" | "inputs" | "io" | "in" => Color::Rgb(  0, 172, 193),
        // ── folder-salt ───────────────────────────────────────────────────
        "salt" | "saltstack"             => Color::Rgb(  3, 169, 244),
        // ── folder-simulations ────────────────────────────────────────────
        "simulations" | "simulation" | "sim" | "sims"
                                         => Color::Rgb(171,  71, 188),
        // ── folder-metro ──────────────────────────────────────────────────
        "metro"                          => Color::Rgb(239,  83,  80),
        // ── folder-filter ─────────────────────────────────────────────────
        "filter" | "filters"             => Color::Rgb(126,  87, 194),
        // ── folder-toc ────────────────────────────────────────────────────
        "toc" | "table-of-contents"      => Color::Rgb( 33, 150, 243),
        // ── folder-cue ────────────────────────────────────────────────────
        "cue" | "cues"                   => Color::Rgb( 68, 138, 255),
        // ── folder-license ────────────────────────────────────────────────
        "license" | "licenses"           => Color::Rgb(255,  87,  34),
        // ── folder-form ───────────────────────────────────────────────────
        "form" | "forms"                 => Color::Rgb(156,  39, 176),
        // ── folder-skills ─────────────────────────────────────────────────
        "skill" | "skills"               => Color::Rgb(255, 143,   0),
        // ── folder-instructions (clone: folder-meta + cyan-A700) ──────────
        "instruction" | "instructions"   => Color::Rgb(  0, 229, 255),

        // ── Misc fallbacks not in material theme ──────────────────────────
        "gateway" | "gateways"           => Color::Rgb(103,  58, 183),
        "session" | "sessions"           => Color::Rgb( 38, 166, 154),
        "users" | "user" | "accounts" | "account" | "profiles" | "profile"
        | "members" | "member"           => Color::Rgb( 30, 136, 229),
        "deploy" | "deployments" | "deployment" | "releases" | "release"
                                         => Color::Rgb( 57, 168,  80),
        "workflows" | "workflow"         => Color::Rgb( 30, 136, 229),
        "actions" | "action"             => Color::Rgb( 35, 134,  54),
        "workers" | "worker"             => Color::Rgb(255, 160,   0),
        "observers" | "observer" | "watchers" | "watcher"
                                         => Color::Rgb(  0, 150, 136),
        "cron" | "crons" | "scheduler" | "schedules" | "schedule"
                                         => Color::Rgb(255, 193,   7),
        "notifications" | "notification" | "alerts" | "alert"
                                         => Color::Rgb(255, 167,  38),
        "sockets" | "socket" | "websockets" | "websocket" | "ws"
                                         => Color::Rgb(  0, 188, 212),
        "queries" | "query"              => Color::Rgb( 66, 133, 244),
        "mutations" | "mutation"         => Color::Rgb(233,  30,  99),
        "subscriptions" | "subscription" => Color::Rgb(229,  57, 172),
        "factories" | "factory"          => Color::Rgb( 66, 133, 244),
        "snapshot"                       => Color::Rgb(141, 110,  99),
        "unit"                           => Color::Rgb(  0, 191, 165),
        "playwright" | "selenium"        => Color::Rgb( 45,  52,  54),
        "legacy" | "deprecated" | "old"  => Color::Rgb( 90,  90,  90),
        "swagger" | "openapi"            => Color::Rgb( 76, 175,  80),
        "datasets" | "dataset"           => Color::Rgb( 30, 136, 229),
        "weights"                        => Color::Rgb(255, 160,   0),
        "experiments" | "experiment" | "runs"
                                         => Color::Rgb(156,  39, 176),
        "mixins" | "mixin"               => Color::Rgb( 65, 184, 131),
        "reducers" | "reducer" | "selectors" | "selector"
                                         => Color::Rgb(126,  87, 194),
        "adapters" | "adapter" | "connectors" | "connector"
                                         => Color::Rgb(  0, 150, 136),
        "transformers" | "transformer" | "transforms" | "transform"
                                         => Color::Rgb(  0, 150, 136),
        "analytics" | "metrics" | "telemetry"
                                         => Color::Rgb( 67, 160,  71),
        "monitoring" | "health" | "healthcheck"
                                         => Color::Rgb( 76, 175,  80),
        "tracing" | "traces" | "trace"   => Color::Rgb(100, 100, 100),
        "audit" | "audits"               => Color::Rgb(211,  47,  47),
        "exceptions" | "exception"       => Color::Rgb(239,  83,  80),
        "internal"                       => Color::Rgb( 30,  80, 160),
        "cmd"                            => Color::Rgb(  3, 169, 244),

        _ => {
            // Prefix/suffix patterns
            if lower.starts_with("config") || lower.ends_with("config") {
                return Color::Rgb(  0, 172, 193);
            }
            if lower.starts_with("component") || lower.ends_with("components") {
                return Color::Rgb(192, 202,  51);
            }
            if lower.starts_with("test_") || lower.ends_with("_test") || lower.ends_with("_spec") {
                return Color::Rgb(  0, 191, 165);
            }
            if lower.ends_with("_docs") || lower.ends_with("-docs") {
                return Color::Rgb(  2, 119, 189);
            }
            if lower.ends_with("_scripts") {
                return Color::Rgb( 84, 110, 122);
            }
            if lower.starts_with("api_") || lower.ends_with("_api") {
                return Color::Rgb(251, 192,  45);
            }
            if lower.ends_with("_utils") || lower.ends_with("_helpers") {
                return Color::Rgb(124, 179,  66);
            }
            Color::Rgb(130, 130, 130) // default gray
        }
    }
}

/// Returns the brand/accent color for the icon of the given file entry.
/// `is_dir` should reflect the resolved target type (symlinks already follow target).
fn nf_entry_icon_color(name: &str, is_dir: bool) -> Color {
    if is_dir {
        let lower = name.to_ascii_lowercase();
        return folder_color(&lower);
    }
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    match lower.as_str() {
        "dockerfile" | "containerfile"
        | "docker-compose.yml" | "docker-compose.yaml" => return Color::Rgb(13, 183, 237),
        "makefile" | "gnumakefile" | "bsdmakefile" | "cmakelists.txt" => {
            return Color::Rgb(100, 180, 100)
        }
        "cargo.toml" | "cargo.lock" => return Color::Rgb(222, 100, 42),
        "package.json" | "package-lock.json" => return Color::Rgb(203, 185, 8),
        "tsconfig.json" | "jsconfig.json" => return Color::Rgb(49, 120, 198),
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => {
            return Color::Rgb(240, 80, 50)
        }
        "license" | "licence" | "license.md" | "licence.md" | "license.txt" => {
            return Color::Rgb(170, 170, 170)
        }
        "readme" | "readme.md" | "readme.txt" | "readme.rst" => {
            return Color::Rgb(68, 139, 241)
        }
        "vagrantfile" | "gemfile" | "rakefile" | "podfile" => return Color::Rgb(185, 49, 42),
        "brewfile" => return Color::Rgb(245, 166, 35),
        _ => {}
    }

    match ext {
        "rs" | "rlib" => Color::Rgb(222, 100, 42),                   // Rust orange
        "py" | "pyw" | "pyi" | "pyc" => Color::Rgb(255, 212, 59),    // Python yellow
        "js" | "mjs" | "cjs" => Color::Rgb(240, 219, 79),            // JS yellow
        "ts" | "cts" | "mts" => Color::Rgb(49, 120, 198),            // TS blue
        "jsx" => Color::Rgb(97, 218, 251),                            // React cyan
        "tsx" => Color::Rgb(97, 218, 251),
        "html" | "htm" | "xhtml" => Color::Rgb(228, 77, 38),         // HTML5 orange
        "css" => Color::Rgb(38, 121, 228),                            // CSS blue
        "scss" | "sass" => Color::Rgb(205, 103, 153),                 // Sass pink
        "less" => Color::Rgb(29, 54, 95),
        "go" => Color::Rgb(0, 173, 216),                              // Go cyan
        "rb" | "erb" | "gemspec" => Color::Rgb(185, 49, 42),         // Ruby red
        "java" => Color::Rgb(248, 152, 32),                           // Java orange
        "kt" | "kts" => Color::Rgb(127, 82, 255),                    // Kotlin purple
        "scala" | "sc" => Color::Rgb(220, 50, 47),
        "groovy" | "gvy" | "gradle" => Color::Rgb(100, 180, 100),
        "clj" | "cljs" | "cljc" | "edn" => Color::Rgb(99, 189, 74),  // Clojure green
        "c" | "h" => Color::Rgb(85, 86, 148),                        // C blue-purple
        "cpp" | "cxx" | "cc" | "c++" | "hpp" | "hxx" | "hh" => Color::Rgb(0, 89, 157),
        "cs" | "csx" => Color::Rgb(104, 33, 122),                    // C# purple
        "m" | "mm" => Color::Rgb(85, 86, 148),                       // ObjC
        "swift" => Color::Rgb(240, 81, 56),                          // Swift orange-red
        "hs" | "lhs" => Color::Rgb(94, 80, 134),                     // Haskell purple
        "ex" | "exs" | "heex" => Color::Rgb(100, 55, 110),           // Elixir purple
        "erl" | "hrl" => Color::Rgb(186, 50, 50),
        "elm" => Color::Rgb(96, 181, 204),                           // Elm blue
        "ml" | "mli" | "fs" | "fsi" | "fsx" => Color::Rgb(55, 98, 161),
        "zig" => Color::Rgb(247, 163, 26),                           // Zig yellow
        "dart" => Color::Rgb(0, 180, 216),                           // Dart blue
        "lua" => Color::Rgb(0, 0, 200),                              // Lua dark blue
        "nim" | "nims" => Color::Rgb(255, 213, 0),                   // Nim yellow
        "cr" => Color::Rgb(0, 0, 0),                                 // Crystal black (on dark bg: white)
        "sh" | "bash" | "bats" | "zsh" | "fish" | "ps1" | "psm1" | "psd1" => {
            Color::Rgb(137, 224, 81)                                  // Shell green
        }
        "json" | "json5" | "jsonc" => Color::Rgb(203, 185, 8),
        "yaml" | "yml" => Color::Rgb(203, 23, 30),                   // YAML red
        "toml" => Color::Rgb(156, 100, 60),                          // TOML brown
        "xml" | "xaml" | "svg" => Color::Rgb(255, 165, 0),          // XML orange
        "ini" | "cfg" | "conf" | "config" | "env" | "properties" | "props" => {
            Color::Rgb(170, 170, 170)
        }
        "lock" => Color::Rgb(200, 150, 50),
        "md" | "mdx" | "markdown" => Color::Rgb(68, 139, 241),       // Markdown blue
        "txt" | "text" | "rst" | "rest" | "adoc" | "asciidoc" => Color::Rgb(180, 180, 180),
        "pdf" => Color::Rgb(224, 50, 50),                            // PDF red
        "graphql" | "gql" | "prisma" => Color::Rgb(229, 53, 171),   // GraphQL pink
        "tf" | "tfvars" | "hcl" => Color::Rgb(92, 67, 209),         // Terraform purple
        "nix" => Color::Rgb(126, 186, 228),                          // NixOS light blue
        "ipynb" => Color::Rgb(240, 100, 0),
        "r" | "rmd" => Color::Rgb(39, 109, 195),                    // R blue
        "jl" => Color::Rgb(149, 88, 178),                            // Julia purple
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff" | "heic" => {
            Color::Rgb(200, 150, 220)
        }
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "flv" | "wmv" => Color::Rgb(180, 60, 180),
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" => Color::Rgb(100, 180, 240),
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "7z" | "rar" | "tgz" => {
            Color::Rgb(200, 160, 80)
        }
        "doc" | "docx" => Color::Rgb(43, 87, 154),
        "xls" | "xlsx" => Color::Rgb(33, 115, 70),
        "ppt" | "pptx" => Color::Rgb(184, 71, 44),
        "diff" | "patch" => Color::Rgb(170, 170, 60),
        "proto" => Color::Rgb(100, 150, 220),
        _ => Color::Rgb(140, 140, 140),
    }
}

/// Returns a plain fallback icon (ASCII arrow) for when Nerd Fonts are off.
fn ascii_entry_icon(is_dir: bool, expanded: bool) -> &'static str {
    if is_dir {
        if expanded { "▾ " } else { "▸ " }
    } else {
        "  "
    }
}
const UNFOCUSED_BORDER: Color = Color::DarkGray;
const SCROLLBAR_THUMB_FOCUSED: Color = Color::Cyan;
const SCROLLBAR_THUMB_UNFOCUSED: Color = Color::Rgb(120, 120, 120);
const SCROLLBAR_TRACK: Color = Color::Rgb(60, 60, 60);

/// Convert a vt100 Color to a ratatui Color.
fn vt100_color(c: vt100::Color) -> Option<Color> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) => Some(Color::Indexed(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

/// Check if a cell (row, col) is within a selection range.
fn in_selection(sel: &crate::app::TextSelection, row: u16, col: u16, cols: u16) -> bool {
    let (sr, sc, er, ec) = if sel.start_row < sel.end_row
        || (sel.start_row == sel.end_row && sel.start_col <= sel.end_col)
    {
        (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
    } else {
        (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
    };
    if row < sr || row > er {
        return false;
    }
    let line_start = if row == sr { sc } else { 0 };
    let line_end = if row == er {
        ec
    } else {
        cols.saturating_sub(1)
    };
    col >= line_start && col <= line_end
}

/// Build ratatui Text directly from vt100 screen cells.
fn vt100_screen_to_text(
    screen: &vt100::Screen,
    selection: Option<&crate::app::TextSelection>,
) -> Text<'static> {
    let (rows, cols) = screen.size();
    let mut lines = Vec::with_capacity(rows as usize);

    for row in 0..rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut buf = String::new();
        let mut cur_style = Style::default();
        let mut col = 0u16;

        while col < cols {
            let cell = match screen.cell(row, col) {
                Some(c) => c,
                None => {
                    col += 1;
                    continue;
                }
            };

            if cell.is_wide_continuation() {
                col += 1;
                continue;
            }

            let mut style = Style::default();
            if let Some(fg) = vt100_color(cell.fgcolor()) {
                style = style.fg(fg);
            }
            if let Some(bg) = vt100_color(cell.bgcolor()) {
                style = style.bg(bg);
            }
            if cell.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            if cell.dim() {
                style = style.add_modifier(Modifier::DIM);
            }
            if cell.italic() {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline() {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse() {
                style = style.add_modifier(Modifier::REVERSED);
            }

            // Apply selection highlight
            if let Some(sel) = selection {
                if in_selection(sel, row, col, cols) {
                    style = style.bg(Color::Rgb(60, 80, 140));
                }
            }

            let ch = cell.contents();

            if style != cur_style && !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), cur_style));
            }
            cur_style = style;

            if ch.is_empty() {
                buf.push(' ');
            } else {
                buf.push_str(ch);
            }

            col += if cell.is_wide() { 2 } else { 1 };
        }

        if !buf.is_empty() {
            spans.push(Span::styled(buf, cur_style));
        }

        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

/// Render a scrollbar on the right edge of a panel area.
fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    is_narrow: bool,
    scroll_offset: u16,
    max_scroll: u16,
    focused: bool,
) {
    if max_scroll == 0 || area.height < 3 {
        return;
    }

    let border_top: u16 = 1;
    let border_bottom: u16 = if is_narrow { 0 } else { 1 };
    let track_height = area
        .height
        .saturating_sub(border_top + border_bottom)
        .max(1) as usize;
    if track_height < 2 {
        return;
    }

    let total_content = max_scroll + track_height as u16;
    let thumb_size = ((track_height as f64 * track_height as f64) / total_content as f64)
        .ceil()
        .max(1.0)
        .min(track_height as f64) as usize;

    let scrollable = track_height.saturating_sub(thumb_size);
    let thumb_pos = if max_scroll > 0 {
        ((scroll_offset as f64 / max_scroll as f64) * scrollable as f64).round() as usize
    } else {
        0
    };

    let thumb_color = if focused {
        SCROLLBAR_THUMB_FOCUSED
    } else {
        SCROLLBAR_THUMB_UNFOCUSED
    };

    let bar_x = area.x + area.width.saturating_sub(1);
    let bar_y_start = area.y + border_top;

    let buf = frame.buffer_mut();
    for i in 0..track_height {
        let y = bar_y_start + i as u16;
        if y >= area.y + area.height.saturating_sub(border_bottom) {
            break;
        }
        let is_thumb = i >= thumb_pos && i < thumb_pos + thumb_size;
        let ch = if is_thumb { "┃" } else { "│" };
        let style = if is_thumb {
            Style::default().fg(thumb_color)
        } else {
            Style::default().fg(SCROLLBAR_TRACK)
        };
        if let Some(cell) = buf.cell_mut((bar_x, y)) {
            cell.set_symbol(ch);
            cell.set_style(style);
        }
    }

    // Scroll position overlay indicator at top-right
    if scroll_offset > 0 {
        let pct = (scroll_offset as f64 / max_scroll as f64 * 100.0) as u16;
        let indicator = format!(" {}% ", pct);
        let ind_w = indicator.width() as u16;
        let ind_x = area.x + area.width.saturating_sub(ind_w + 1);
        let ind_y = area.y;
        if ind_w + 1 < area.width {
            let ind_area = Rect::new(ind_x, ind_y, ind_w, 1);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    indicator,
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )),
                ind_area,
            );
        }
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();

    let is_narrow = size.width < 100;
    let status_height = if is_narrow { 2 } else { 1 };
    let tab_height: u16 = 1;

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(status_height)])
        .split(size);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    let tab_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(tab_height), Constraint::Min(1)])
        .split(content_area);

    app.tab_bar_area = tab_chunks[0];
    draw_tabs(frame, app, tab_chunks[0], is_narrow);
    let body_area = tab_chunks[1];

    if app.is_on_welcome() {
        app.output_area = Rect::default();
        app.git_status_area = Rect::default();
        app.git_log_area = Rect::default();
        app.file_browser_area = Rect::default();
        draw_splash(frame, app, body_area);
        draw_status_bar(frame, app, status_area);
        if app.show_picker || app.show_command_palette {
            draw_command_palette(frame, app, size);
        }
        return;
    }

    // Calculate layout with optional file browser on left
    let file_browser_width: u16 = if app.show_file_browser && !is_narrow {
        (size.width / 5).clamp(20, 40)
    } else {
        0
    };

    let main_body = if file_browser_width > 0 {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(file_browser_width), Constraint::Min(1)])
            .split(body_area);
        app.file_browser_area = h_chunks[0];
        // File browser drawn AFTER other panels (below) to prevent overlap artifacts
        h_chunks[1]
    } else {
        app.file_browser_area = Rect::default();
        body_area
    };

    // Build layout: all panels can coexist independently
    // [file_viewer?] [terminal] [git_panel?]
    if is_narrow {
        // Narrow: only one panel at a time, priority: git > file_view > terminal
        app.file_viewer_area = Rect::default();
        app.output_area = Rect::default();
        app.git_status_area = Rect::default();
        app.git_log_area = Rect::default();

        if app.show_right_panel {
            draw_right_panel(frame, app, main_body, is_narrow);
        } else if app.viewing_file.is_some() && app.show_file_browser && app.show_file_view {
            app.file_viewer_area = main_body;
            draw_file_viewer(frame, app, main_body, is_narrow);
        } else {
            app.output_area = main_body;
            draw_claude_output(frame, app, main_body, is_narrow);
        }
    } else {
        // Wide: build constraints dynamically based on visible panels
        // File viewer only shows when file browser is open and a file is selected
        let has_file_viewer = app.viewing_file.is_some() && app.show_file_browser;
        let has_git = app.show_right_panel;

        // Reset areas
        app.file_viewer_area = Rect::default();
        app.output_area = Rect::default();
        app.git_status_area = Rect::default();
        app.git_log_area = Rect::default();

        // Determine column count and constraints
        let mut constraints: Vec<Constraint> = Vec::new();
        // Track what each column index maps to
        // 0 = file_viewer, 1 = terminal, 2 = git_panel
        let mut columns: Vec<u8> = Vec::new();

        if has_file_viewer {
            constraints.push(Constraint::Percentage(if has_git { 35 } else { 50 }));
            columns.push(0);
        }
        // Terminal is always shown in wide mode
        constraints.push(Constraint::Min(20));
        columns.push(1);
        if has_git {
            constraints.push(Constraint::Percentage(30));
            columns.push(2);
        }

        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(main_body);

        for (i, &col_type) in columns.iter().enumerate() {
            match col_type {
                0 => {
                    app.file_viewer_area = h_chunks[i];
                    draw_file_viewer(frame, app, h_chunks[i], is_narrow);
                }
                1 => {
                    app.output_area = h_chunks[i];
                    draw_claude_output(frame, app, h_chunks[i], is_narrow);
                }
                2 => {
                    draw_right_panel(frame, app, h_chunks[i], is_narrow);
                }
                _ => {}
            }
        }
    }

    // Draw file browser after other panels
    if file_browser_width > 0 && !is_narrow {
        draw_file_browser(frame, app, app.file_browser_area, is_narrow);
    }

    draw_status_bar(frame, app, status_area);

    // Overlays — render a dimmed backdrop before each modal
    if app.show_close_confirm {
        draw_backdrop(frame, size);
        draw_confirm_dialog(frame, size);
    }
    if app.show_picker || app.show_command_palette {
        draw_backdrop(frame, size);
        draw_command_palette(frame, app, size);
    }
    if app.show_settings {
        draw_backdrop(frame, size);
        draw_settings(frame, app, size);
    }

    // Error message overlay
    if let Some(ref msg) = app.error_message {
        let lines: Vec<Line> = msg
            .lines()
            .map(|l| Line::from(Span::styled(l, Style::default().fg(Color::White))))
            .collect();
        let height = (lines.len() as u16 + 2).min(size.height);
        let width = (msg.len() as u16 + 4).min(size.width).max(30);
        let area = Rect {
            x: size.width.saturating_sub(width) / 2,
            y: size.height.saturating_sub(height) / 2,
            width,
            height,
        };
        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Red))
            .style(Style::default().bg(Color::Black));
        let para = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(para, area);
    }

    // File browser overlay on narrow mode
    if app.show_file_browser && is_narrow {
        let overlay_w = (size.width * 3 / 4).min(size.width);
        let overlay_area = Rect::new(0, tab_height, overlay_w, body_area.height);
        app.file_browser_area = overlay_area;
        frame.render_widget(ratatui::widgets::Clear, overlay_area);
        draw_file_browser(frame, app, overlay_area, is_narrow);
    }
}

fn focused_block(title: &str, focused: bool) -> Block<'_> {
    let border_color = if focused {
        FOCUSED_BORDER
    } else {
        UNFOCUSED_BORDER
    };
    let title_style = if focused {
        Style::default()
            .fg(FOCUSED_BORDER)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(UNFOCUSED_BORDER)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title, title_style))
}

fn draw_splash(frame: &mut Frame, app: &App, area: Rect) {
    let typing_indicator = if app.is_typing() { " ●" } else { "" };

    // Big logo needs ~40 cols wide and ~14 rows tall (logo + subtitle + hints + padding)
    let use_big_logo = area.width >= 45 && area.height >= 14;

    let mut lines: Vec<Line> = Vec::new();

    if use_big_logo {
        let logo = vec![
            "",
            "       ██████╗ ██╗██████╗ ███████╗",
            "       ██╔══██╗██║██╔══██╗██╔════╝",
            "       ███████║██║██║  ██║█████╗  ",
            "       ██╔══██║██║██║  ██║██╔══╝  ",
            "       ██║  ██║██║██████╔╝███████╗",
            "       ╚═╝  ╚═╝╚═╝╚═════╝ ╚══════╝",
            "",
        ];

        let content_height = logo.len() + 8;
        let v_pad = (area.height as usize).saturating_sub(content_height) / 2;
        for _ in 0..v_pad {
            lines.push(Line::from(""));
        }

        for l in &logo {
            lines.push(Line::from(Span::styled(
                *l,
                Style::default().fg(Color::Cyan),
            )));
        }
    } else {
        // Compact: just the name, centered vertically
        let content_height: usize = 9;
        let v_pad = (area.height as usize).saturating_sub(content_height) / 2;
        for _ in 0..v_pad {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "  ── aide ──",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        format!("  Terminal IDE for Claude Code{}", typing_indicator),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Keybind hints
    let keybinds: Vec<(&str, &str)> = vec![
        ("^P", "Command Palette"),
        ("^T", "New Tab"),
        ("^G", "Toggle Git Panel"),
        ("^B", "Toggle File Browser"),
        ("^X", "Quit"),
    ];

    for (key, label) in &keybinds {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:>4} ", key),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(*label, Style::default().fg(Color::DarkGray)),
        ]));
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect, _is_narrow: bool) {
    let on_welcome = app.is_on_welcome();

    let mut titles: Vec<(String, bool, bool)> = app
        .session_manager
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_active = !on_welcome && i == app.session_manager.active_index;
            let label = if s.has_notification && !is_active {
                format!("* {}", s.name)
            } else {
                s.name.clone()
            };
            (label, is_active, s.has_notification && !is_active)
        })
        .collect();

    if app.show_welcome || app.session_manager.sessions.is_empty() {
        titles.push(("aide".to_string(), on_welcome, false));
    }

    let selected = if on_welcome {
        titles.len().saturating_sub(1)
    } else {
        app.session_manager.active_index
    };

    let block = Block::default().borders(Borders::NONE);

    let divider = " ";
    let divider_w = divider.width();

    let inner_w = area.width as usize;
    let tab_widths: Vec<usize> = titles
        .iter()
        .map(|(label, _, _)| label.width() + 2)
        .collect();
    let arrow_w = 2;
    let total_w: usize =
        tab_widths.iter().sum::<usize>() + titles.len().saturating_sub(1) * divider_w;

    let mut start = app.tab_scroll_offset;
    #[allow(unused_assignments)]
    let mut end = titles.len();

    let needs_overflow = total_w > inner_w;

    if needs_overflow && !titles.is_empty() {
        if selected < start {
            start = selected;
        }

        end = start;
        let mut used = 0usize;
        #[allow(clippy::needless_range_loop)]
        for i in start..titles.len() {
            let left_space = if start > 0 { arrow_w } else { 0 };
            let right_space = arrow_w;
            let budget = inner_w.saturating_sub(left_space + right_space);

            let cost = if i == start {
                tab_widths[i]
            } else {
                divider_w + tab_widths[i]
            };
            if used + cost > budget && i > selected {
                break;
            }
            used += cost;
            end = i + 1;
        }

        if selected >= end {
            end = selected + 1;
            used = tab_widths[selected];
            start = selected;
            while start > 0 {
                let left_space = if start - 1 > 0 { arrow_w } else { 0 };
                let right_space = if end < titles.len() { arrow_w } else { 0 };
                let budget = inner_w.saturating_sub(left_space + right_space);
                let cost = divider_w + tab_widths[start - 1];
                if used + cost > budget {
                    break;
                }
                used += cost;
                start -= 1;
            }
        }

        if end >= titles.len() {
            let left_space = if start > 0 { arrow_w } else { 0 };
            let budget = inner_w.saturating_sub(left_space);
            let mut recalc_used: usize = tab_widths[start..end].iter().sum::<usize>()
                + (end - start).saturating_sub(1) * divider_w;
            while start > 0 {
                let new_left = if start - 1 > 0 { arrow_w } else { 0 };
                let cost = divider_w + tab_widths[start - 1];
                if recalc_used + cost > inner_w.saturating_sub(new_left) {
                    break;
                }
                recalc_used += cost;
                start -= 1;
            }
            let _ = budget;
        }

        app.tab_scroll_offset = start;
    } else {
        start = 0;
        end = titles.len();
        app.tab_scroll_offset = 0;
    }

    let has_left = start > 0;
    let has_right = end < titles.len();

    let visible_titles: Vec<&(String, bool, bool)> = titles[start..end].iter().collect();
    let visible_selected = selected.saturating_sub(start);

    let mut spans: Vec<Span> = Vec::new();
    let mut tab_click_zones: Vec<(u16, u16, usize)> = Vec::new();
    let mut cursor_x = area.x;

    if has_left {
        spans.push(Span::styled(
            "◀ ",
            Style::default()
                .fg(Color::DarkGray)
                .bg(Color::Rgb(30, 30, 30)),
        ));
        cursor_x += 2;
    }

    for (i, (label, _is_active, has_notif)) in visible_titles.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                divider,
                Style::default().bg(Color::Rgb(30, 30, 30)),
            ));
            cursor_x += divider_w as u16;
        }
        let is_sel = i == visible_selected;
        let tab_text = format!(" {} ", label);
        let tab_w = tab_text.width() as u16;

        let tab_index = start + i;
        tab_click_zones.push((cursor_x, cursor_x + tab_w, tab_index));

        let style = if is_sel {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(60, 60, 80))
                .add_modifier(Modifier::BOLD)
        } else if *has_notif {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(30, 30, 30))
        } else {
            Style::default()
                .fg(Color::Rgb(140, 140, 140))
                .bg(Color::Rgb(30, 30, 30))
        };

        spans.push(Span::styled(tab_text, style));
        cursor_x += tab_w;
    }

    // Fill remaining space with background
    let remaining = (area.width as usize)
        .saturating_sub(cursor_x.saturating_sub(area.x) as usize + if has_right { 2 } else { 0 });
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().bg(Color::Rgb(30, 30, 30)),
        ));
    }

    if has_right {
        spans.push(Span::styled(
            " ▶",
            Style::default()
                .fg(Color::DarkGray)
                .bg(Color::Rgb(30, 30, 30)),
        ));
    }

    app.tab_click_zones = tab_click_zones;

    let tab_line = Line::from(spans);
    let paragraph = Paragraph::new(tab_line).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_claude_output(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::Output;

    let border_h: u16 = if is_narrow { 0 } else { 2 };
    let border_w: u16 = if is_narrow { 0 } else { 2 };
    let inner_width = area.width.saturating_sub(border_w);
    let inner_height = area.height.saturating_sub(border_h);
    app.output_width = inner_width;
    app.output_height = inner_height;

    let base_title = if app.pty_title.is_empty() {
        "Terminal".to_string()
    } else {
        app.pty_title.clone()
    };
    let title = if app.is_typing() {
        format!(" {} ● ", base_title)
    } else {
        format!(" {} ", base_title)
    };

    let block = if is_narrow {
        Block::default()
    } else {
        focused_block(&title, is_focused)
    };

    // Use vt100 parser for proper terminal rendering
    if let Some(parser) = &mut app.pty_parser {
        // Resize the parser to match the current viewport before rendering.
        // This ensures the screen dimensions are always consistent with the
        // area we're about to draw into, eliminating one-frame glitches where
        // the parser has stale dimensions from before a terminal resize.
        {
            let screen = parser.screen_mut();
            let (cur_rows, cur_cols) = screen.size();
            if cur_rows != inner_height || cur_cols != inner_width {
                screen.set_size(inner_height, inner_width);
            }
        }

        let screen = parser.screen_mut();

        // Get max scrollback available
        screen.set_scrollback(usize::MAX);
        let max_scrollback = screen.scrollback() as u16;

        // Set desired scroll position
        if app.follow_mode {
            screen.set_scrollback(0);
        } else {
            if app.scroll_offset > max_scrollback {
                app.scroll_offset = max_scrollback;
            }
            screen.set_scrollback(app.scroll_offset as usize);
        }

        // Capture cursor state before releasing screen borrow
        let cursor_pos = screen.cursor_position(); // (row, col)
        let cursor_visible = !screen.hide_cursor();
        let at_bottom = app.follow_mode || app.scroll_offset == 0;

        // Build ratatui Text directly from vt100 cell data
        let text = vt100_screen_to_text(screen, app.selection.as_ref());

        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);

        // Render block cursor (always visible, greyed when unfocused, blinking when focused)
        if cursor_visible && at_bottom {
            // While typing, keep cursor solid; otherwise blink at ~530ms
            let recently_typed = app
                .last_input_time
                .map(|t| t.elapsed().as_millis() < 530)
                .unwrap_or(false);
            let blink_on = if is_focused {
                if recently_typed {
                    true
                } else {
                    let ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    (ms / 530).is_multiple_of(2)
                }
            } else {
                true // unfocused cursor is always visible (solid grey)
            };

            let border = if is_narrow { 0u16 } else { 1 };
            let cx = area.x + border + cursor_pos.1;
            let cy = area.y + border + cursor_pos.0;
            if blink_on
                && cx < area.x + area.width.saturating_sub(border)
                && cy < area.y + area.height.saturating_sub(border)
            {
                let buf = frame.buffer_mut();
                if let Some(cell) = buf.cell_mut((cx, cy)) {
                    if is_focused {
                        // If the cell already has REVERSED modifier, remove it
                        // before swapping — otherwise the double-inversion
                        // makes the cursor invisible on inverse text.
                        let already_reversed =
                            cell.modifier.contains(Modifier::REVERSED);
                        if already_reversed {
                            cell.modifier.remove(Modifier::REVERSED);
                        }
                        let fg = cell.fg;
                        let bg = cell.bg;
                        let new_fg = if bg == Color::Reset { Color::Black } else { bg };
                        let new_bg = if fg == Color::Reset { Color::White } else { fg };
                        cell.fg = new_fg;
                        cell.bg = new_bg;
                    } else {
                        // Grey block for unfocused cursor
                        cell.bg = Color::Rgb(100, 100, 100);
                    }
                }
            }
        }

        // Scrollbar
        if max_scrollback > 0 {
            let scroll_pos = if app.follow_mode {
                max_scrollback
            } else {
                max_scrollback.saturating_sub(app.scroll_offset)
            };
            render_scrollbar(
                frame,
                area,
                is_narrow,
                scroll_pos,
                max_scrollback,
                is_focused,
            );
        }
    } else {
        // Fallback: raw output with ansi_to_tui (before parser is initialized)
        let text = app
            .claude_output
            .as_bytes()
            .to_vec()
            .into_text()
            .unwrap_or_else(|_| Text::raw(&app.claude_output));

        let total_lines = text.lines.len() as u16;
        let max_scroll_back = total_lines.saturating_sub(inner_height);

        if app.scroll_offset > max_scroll_back {
            app.scroll_offset = max_scroll_back;
        }

        let top_offset = if app.follow_mode {
            max_scroll_back
        } else {
            max_scroll_back.saturating_sub(app.scroll_offset)
        };

        let paragraph = Paragraph::new(text).block(block).scroll((top_offset, 0));
        frame.render_widget(paragraph, area);

        let scroll_pos = max_scroll_back.saturating_sub(app.scroll_offset.min(max_scroll_back));
        render_scrollbar(
            frame,
            area,
            is_narrow,
            scroll_pos,
            max_scroll_back,
            is_focused,
        );
    }
}

fn draw_right_panel(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    app.git_status_area = chunks[0];
    app.git_log_area = chunks[1];

    draw_git_status(frame, app, chunks[0], is_narrow);
    draw_git_log(frame, app, chunks[1], is_narrow);
}

fn git_panel_block<'a>(title: &'a str, is_focused: bool, is_narrow: bool) -> Block<'a> {
    let border_color = if is_focused {
        FOCUSED_BORDER
    } else {
        UNFOCUSED_BORDER
    };
    if is_narrow {
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, Style::default().fg(border_color)))
    } else {
        focused_block(title, is_focused)
    }
}

fn draw_git_status(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::GitStatus;
    let border_w: u16 = if is_narrow { 0 } else { 2 };
    let inner_width = area.width.saturating_sub(border_w) as usize;

    // Not a git repo — show empty state
    if app.git_branch.is_empty() {
        let title = " Status ".to_string();
        let block = git_panel_block(&title, is_focused, is_narrow);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                " Not a git repository",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Branch header
    let branch_line = if app.git_remote_branch.is_empty() {
        Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                &app.git_branch,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  (no upstream)", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let (behind, ahead) = app.git_upstream.unwrap_or((0, 0));
        let sync_icon = if behind == 0 && ahead == 0 {
            Span::styled(" ✓", Style::default().fg(Color::Green))
        } else {
            let mut parts = String::new();
            if behind > 0 {
                parts.push_str(&format!(" ↓{}", behind));
            }
            if ahead > 0 {
                parts.push_str(&format!(" ↑{}", ahead));
            }
            Span::styled(parts, Style::default().fg(Color::Yellow))
        };
        Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                &app.git_branch,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" → ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.git_remote_branch, Style::default().fg(Color::DarkGray)),
            sync_icon,
        ])
    };
    lines.push(branch_line);
    lines.push(Line::from(""));

    // Parse status lines: [filename] [flex space] +added -removed [A/M/D]
    let mut file_row_idx: usize = 0;
    for line in app.git_status.lines() {
        if line.starts_with("##") {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (index_status, worktree_status) = if line.len() >= 2 {
            (
                line.chars().next().unwrap_or(' '),
                line.chars().nth(1).unwrap_or(' '),
            )
        } else {
            (' ', ' ')
        };

        let filename = if line.len() > 3 { &line[3..] } else { trimmed };

        // Determine the primary status letter and color
        let (status_char, status_color) = match (index_status, worktree_status) {
            ('?', '?') => ('U', Color::DarkGray),  // untracked — 'U' not '?'
            ('A', _) | (_, 'A') => ('A', Color::Green),
            ('D', _) | (_, 'D') => ('D', Color::Red),
            ('R', _) => ('R', Color::Magenta),
            ('M', _) | (_, 'M') => ('M', Color::Yellow),
            ('C', _) => ('C', Color::Cyan),
            _ => ('U', Color::DarkGray),
        };

        // Get per-file diff stats
        let (file_added, file_removed) =
            app.git_file_stats.get(filename).copied().unwrap_or((0, 0));

        let added_str = format!("+{}", file_added);
        let removed_str = format!("-{}", file_removed);
        let status_str = format!(" {}", status_char);

        // Split filename into basename + parent dir
        let is_dir = filename.ends_with('/');
        let bare_path = filename.trim_end_matches('/');
        let (file_basename, file_dir) = if let Some(pos) = bare_path.rfind('/') {
            (&bare_path[pos + 1..], &bare_path[..pos + 1])
        } else {
            (bare_path, "")
        };

        let (icon, icon_color) = if app.icons {
            (
                nf_entry_icon(file_basename, is_dir, false),
                nf_entry_icon_color(file_basename, is_dir),
            )
        } else {
            ("", Color::Reset)
        };
        // icon has one trailing space; add second for breathing room
        let icon_display = if app.icons { format!("{}  ", icon.trim_end()) } else { String::new() };
        let icon_w = icon_display.width();

        // Layout: " [icon]  [basename] [dir]  [pad]  [+N] [-N] [S] "
        let prefix_w = 1usize;
        let basename_w = file_basename.width();
        let dir_w = if file_dir.is_empty() { 0 } else { file_dir.width() + 1 };
        let suffix_w = added_str.width() + 1 + removed_str.width() + status_str.width() + 1;
        let used = prefix_w + icon_w + basename_w + dir_w + suffix_w + 1;
        let pad = inner_width.saturating_sub(used).max(1);

        let mut row_spans = vec![
            Span::raw(" "),
            Span::styled(icon_display, Style::default().fg(icon_color)),
            Span::styled(file_basename.to_string(), Style::default().fg(Color::White)),
        ];
        if !file_dir.is_empty() {
            row_spans.push(Span::raw(" "));
            row_spans.push(Span::styled(
                file_dir.to_string(),
                Style::default().fg(Color::Rgb(100, 100, 120)),
            ));
        }
        row_spans.push(Span::raw(" ".repeat(pad)));
        row_spans.push(Span::styled(added_str, Style::default().fg(Color::Green)));
        row_spans.push(Span::raw(" "));
        row_spans.push(Span::styled(removed_str, Style::default().fg(Color::Red)));
        row_spans.push(Span::styled(
            status_str,
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ));
        row_spans.push(Span::raw(" "));
        let is_sel = app.git_status_selected == Some(file_row_idx);
        let row_line = if is_sel {
            Line::from(row_spans).style(Style::default().bg(Color::Rgb(40, 40, 65)))
        } else {
            Line::from(row_spans)
        };
        lines.push(row_line);
        file_row_idx += 1;
    }

    if lines.len() <= 2 {
        lines.push(Line::from(Span::styled(
            " ✓ Working tree clean",
            Style::default().fg(Color::Green),
        )));
    }

    let border_overhead: u16 = if is_narrow { 1 } else { 2 };
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(border_overhead);
    let max_scroll = total.saturating_sub(visible);
    app.git_status_scroll = app.git_status_scroll.min(max_scroll);

    let title = " Status ".to_string();
    let block = git_panel_block(&title, is_focused, is_narrow);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_status_scroll, 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar
    render_scrollbar(
        frame,
        area,
        is_narrow,
        app.git_status_scroll,
        max_scroll,
        is_focused,
    );
}

fn draw_git_log(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    use crate::app::GitLogRow;

    let is_focused = app.focus == FocusPanel::GitLog;
    let border_w: u16 = if is_narrow { 0 } else { 2 };
    let inner_width = area.width.saturating_sub(border_w) as usize;

    // Not a git repo — show empty state
    if app.git_branch.is_empty() {
        app.git_log_rows.clear();
        let title = " Log ".to_string();
        let block = git_panel_block(&title, is_focused, is_narrow);
        let paragraph = Paragraph::new("").block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut row_map: Vec<GitLogRow> = Vec::new();

    for line in app.git_log.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            lines.push(Line::from(""));
            row_map.push(GitLogRow::Graph);
            continue;
        }

        // Extract graph prefix (*, |, /, \, spaces)
        let mut graph_end = 0;
        for (i, ch) in line.char_indices() {
            if matches!(ch, '*' | '|' | '/' | '\\' | ' ') {
                graph_end = i + ch.len_utf8();
            } else {
                break;
            }
        }

        let graph_part = &line[..graph_end];
        let rest = &line[graph_end..];
        let mut spans: Vec<Span> = Vec::new();

        // Color graph characters — replace '*' commit bullet with NF circle icon
        let mut graph_str = String::new();
        for ch in graph_part.chars() {
            match ch {
                '*' => {
                    if !graph_str.is_empty() {
                        spans.push(Span::styled(
                            std::mem::take(&mut graph_str),
                            Style::default().fg(Color::Rgb(80, 80, 120)),
                        ));
                    }
                    spans.push(Span::styled(
                        "\u{f444}", // nf-md-circle_small ●
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ));
                }
                _ => graph_str.push(ch),
            }
        }
        if !graph_str.is_empty() {
            spans.push(Span::styled(
                graph_str,
                Style::default().fg(Color::Rgb(80, 80, 120)),
            ));
        }

        if rest.is_empty() {
            lines.push(Line::from(spans));
            row_map.push(GitLogRow::Graph);
            continue;
        }

        // Parse: hash [decoration] message (time)
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.is_empty() {
            spans.push(Span::raw(rest.to_string()));
            lines.push(Line::from(spans));
            row_map.push(GitLogRow::Graph);
            continue;
        }

        let hash = parts[0].to_string();
        let expanded = app.expanded_commits.contains(&hash);

        // Hash — yellow
        spans.push(Span::styled(
            hash.clone(),
            Style::default().fg(Color::Yellow),
        ));

        // Expand indicator ▼/►
        let indicator = if expanded { " ▼" } else { " ►" };
        spans.push(Span::styled(
            indicator,
            Style::default().fg(Color::Rgb(100, 100, 140)),
        ));

        if parts.len() >= 2 {
            let remainder = parts[1];
            if remainder.starts_with('(') {
                if let Some(close) = remainder.find(')') {
                    let decoration = &remainder[1..close];
                    for ref_name in decoration.split(", ") {
                        let ref_name = ref_name.trim();
                        spans.push(Span::raw(" "));
                        if ref_name == "HEAD" || ref_name.starts_with("HEAD ->") {
                            spans.push(Span::styled(
                                format!(" {} ", ref_name),
                                Style::default()
                                    .fg(Color::Black)
                                    .bg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        } else if ref_name.starts_with("origin/") || ref_name.starts_with("upstream/") {
                            spans.push(Span::styled(
                                format!(" {} ", ref_name),
                                Style::default()
                                    .fg(Color::White)
                                    .bg(Color::Rgb(120, 40, 40)),
                            ));
                        } else if ref_name.starts_with("tag:") {
                            spans.push(Span::styled(
                                format!(" {} ", ref_name),
                                Style::default()
                                    .fg(Color::White)
                                    .bg(Color::Rgb(100, 40, 100))
                                    .add_modifier(Modifier::BOLD),
                            ));
                        } else {
                            spans.push(Span::styled(
                                format!(" {} ", ref_name),
                                Style::default()
                                    .fg(Color::Black)
                                    .bg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        }
                    }
                    spans.push(Span::raw(" "));

                    let after_dec = remainder[close + 1..].trim_start();
                    if let Some(time_start) = after_dec.rfind('(') {
                        let msg = after_dec[..time_start].trim_end();
                        let time_clean = after_dec[time_start + 1..].trim_end_matches(')');
                        let time_str = format!(" {}", time_clean);
                        let used_w: usize = spans.iter().map(|s| s.content.width()).sum();
                        let avail_msg = inner_width.saturating_sub(used_w + time_str.width() + 1);
                        let msg_str = truncate_str(msg, avail_msg);
                        let msg_w = msg_str.width();
                        spans.push(Span::styled(msg_str, Style::default().fg(Color::White)));
                        let total_used = used_w + msg_w + time_str.width();
                        let pad = inner_width.saturating_sub(total_used);
                        if pad > 0 {
                            spans.push(Span::raw(" ".repeat(pad)));
                        }
                        spans.push(Span::styled(time_str, Style::default().fg(Color::DarkGray)));
                    } else {
                        spans.push(Span::styled(after_dec.to_string(), Style::default().fg(Color::White)));
                    }
                } else {
                    spans.push(Span::styled(format!(" {}", remainder), Style::default().fg(Color::White)));
                }
            } else {
                if let Some(time_start) = remainder.rfind('(') {
                    let msg = remainder[..time_start].trim_end();
                    let time_clean = remainder[time_start + 1..].trim_end_matches(')');
                    let time_str = format!(" {}", time_clean);
                    let used_w: usize = spans.iter().map(|s| s.content.width()).sum();
                    let avail_msg = inner_width.saturating_sub(used_w + time_str.width() + 2);
                    let msg_str = truncate_str(msg, avail_msg);
                    let msg_span = format!(" {}", msg_str);
                    let msg_w = msg_span.width();
                    spans.push(Span::styled(msg_span, Style::default().fg(Color::White)));
                    let total_used = used_w + msg_w + time_str.width();
                    let pad = inner_width.saturating_sub(total_used);
                    if pad > 0 {
                        spans.push(Span::raw(" ".repeat(pad)));
                    }
                    spans.push(Span::styled(time_str, Style::default().fg(Color::DarkGray)));
                } else {
                    spans.push(Span::styled(format!(" {}", remainder), Style::default().fg(Color::White)));
                }
            }
        }

        let commit_display_row = lines.len() + app.git_log_scroll as usize;
        let is_commit_sel = app.git_log_selected_row == Some(commit_display_row);
        let commit_line = if is_commit_sel {
            Line::from(spans).style(Style::default().bg(Color::Rgb(40, 40, 65)))
        } else {
            Line::from(spans)
        };
        lines.push(commit_line);
        row_map.push(GitLogRow::Commit(hash.clone()));

        // If expanded, insert a file row for each changed file
        if expanded {
            if let Some(files) = app.commit_files.get(&hash).cloned() {
                if files.is_empty() {
                    let empty_line = Line::from(vec![
                        Span::raw("    "),
                        Span::styled("no files", Style::default().fg(Color::Rgb(80, 80, 80))),
                    ]);
                    lines.push(empty_line);
                    row_map.push(GitLogRow::Graph);
                } else {
                    for (file_idx, file) in files.iter().enumerate() {
                        let path = &file.path;
                        let (filename, rel_dir) = if let Some(pos) = path.rfind('/') {
                            (&path[pos + 1..], &path[..pos + 1])
                        } else {
                            (path.as_str(), "")
                        };

                        let (status_char, status_color) = match file.status {
                            'A' => ("A", Color::Green),
                            'D' => ("D", Color::Red),
                            'R' => ("R", Color::Cyan),
                            'C' => ("C", Color::Cyan),
                            _   => ("M", Color::Yellow),
                        };

                        // Icon
                        let (log_icon, log_icon_color) = if app.icons {
                            (
                                nf_entry_icon(filename, false, false),
                                nf_entry_icon_color(filename, false),
                            )
                        } else {
                            ("", Color::Reset)
                        };
                        // icon has one trailing space; add second for breathing room
                        let log_icon_display = if app.icons {
                            format!("{} ", log_icon)
                        } else {
                            String::new()
                        };
                        let icon_w = log_icon_display.width();

                        // Layout: "    {icon}filename  rel_dir  ···  S"
                        let prefix_w = 4usize; // "    "
                        let filename_w = filename.width();
                        let dir_w = if rel_dir.is_empty() { 0 } else { rel_dir.width() + 2 };
                        let status_w = 1usize;
                        let fixed_w = prefix_w + icon_w + filename_w + dir_w + status_w + 2;
                        let pad = inner_width.saturating_sub(fixed_w);

                        let mut fspans = vec![
                            Span::raw("    "),
                            Span::styled(log_icon_display, Style::default().fg(log_icon_color)),
                            Span::styled(filename.to_string(), Style::default().fg(Color::White)),
                        ];
                        if !rel_dir.is_empty() {
                            fspans.push(Span::raw("  "));
                            fspans.push(Span::styled(rel_dir.to_string(), Style::default().fg(Color::Rgb(110, 110, 130))));
                        }
                        if pad > 0 {
                            fspans.push(Span::raw(" ".repeat(pad)));
                        }
                        fspans.push(Span::styled(
                            status_char.to_string(),
                            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
                        ));

                        let file_display_row = lines.len() + app.git_log_scroll as usize;
                        let is_file_sel = app.git_log_selected_row == Some(file_display_row);
                        let file_line = if is_file_sel {
                            Line::from(fspans).style(Style::default().bg(Color::Rgb(40, 40, 65)))
                        } else {
                            Line::from(fspans)
                        };
                        lines.push(file_line);
                        row_map.push(GitLogRow::File { hash: hash.clone(), file_idx });
                    }
                }
            } else {
                // Files not fetched yet (shouldn't happen since toggle_commit_expand fetches them)
                let loading = Line::from(vec![
                    Span::raw("    "),
                    Span::styled("loading…", Style::default().fg(Color::DarkGray)),
                ]);
                lines.push(loading);
                row_map.push(GitLogRow::Graph);
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " No commits yet",
            Style::default().fg(Color::DarkGray),
        )));
        row_map.push(GitLogRow::Graph);
    }

    app.git_log_rows = row_map;

    let border_overhead: u16 = if is_narrow { 1 } else { 2 };
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(border_overhead);
    let max_scroll = total.saturating_sub(visible);
    app.git_log_scroll = app.git_log_scroll.min(max_scroll);

    let title = " Log ".to_string();
    let block = git_panel_block(&title, is_focused, is_narrow);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_log_scroll, 0));
    frame.render_widget(paragraph, area);

    render_scrollbar(frame, area, is_narrow, app.git_log_scroll, max_scroll, is_focused);
}

/// Truncate a string to fit within max_width, adding "..." if truncated.
fn truncate_str(s: &str, max_width: usize) -> String {
    if max_width < 4 {
        return s.chars().take(max_width).collect();
    }
    if s.width() <= max_width {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut w = 0;
        for ch in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + cw + 3 > max_width {
                result.push_str("...");
                break;
            }
            result.push(ch);
            w += cw;
        }
        result
    }
}

fn tilde_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let on_splash = app.is_on_welcome();
    let w = area.width as usize;
    let is_narrow = area.height >= 2;

    // Build path segment
    let directory = if on_splash {
        "aide".to_string()
    } else if let Some(s) = app.session_manager.active_session() {
        tilde_path(&s.directory)
    } else {
        "~".to_string()
    };

    // Build branch + upstream + diff segment
    let git_spans: Vec<Span> = if on_splash || app.git_branch.is_empty() {
        Vec::new()
    } else {
        let branch = &app.git_branch;
        let (behind, ahead) = app.git_upstream.unwrap_or((0, 0));
        let (added, deleted) = app.git_diff_stats.unwrap_or((0, 0));

        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!(" {} ", branch),
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(40, 40, 40))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" ↓{} ↑{} ", behind, ahead),
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(40, 40, 40)),
        ));
        if added > 0 || deleted > 0 {
            spans.push(Span::styled(
                format!(" +{}", added),
                Style::default().fg(Color::Green).bg(Color::Rgb(40, 40, 40)),
            ));
            spans.push(Span::styled(
                format!(" -{} ", deleted),
                Style::default().fg(Color::Red).bg(Color::Rgb(40, 40, 40)),
            ));
        }
        spans
    };

    // Build background job / status message segment
    let job_spans: Vec<Span> = if let Some(job) = app.bg_jobs.first() {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = (job.started.elapsed().as_millis() / 80) as usize % spinner_frames.len();
        vec![
            Span::styled(
                format!(" {} ", spinner_frames[idx]),
                Style::default().fg(Color::Cyan).bg(Color::Rgb(40, 40, 40)),
            ),
            Span::styled(
                format!("{} ", job.label),
                Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 40)),
            ),
        ]
    } else if let Some((msg, _, is_error)) = &app.status_message {
        let color = if *is_error { Color::Red } else { Color::Green };
        vec![Span::styled(
            format!(" {} ", msg),
            Style::default().fg(color).bg(Color::Rgb(40, 40, 40)),
        )]
    } else {
        Vec::new()
    };

    // Build keybind hints
    let hint_spans: Vec<Span> = if on_splash {
        vec![
            hint_key("^P"),
            hint_label(" commands "),
            hint_key("^X"),
            hint_label(" exit "),
        ]
    } else if matches!(app.focus, FocusPanel::GitStatus | FocusPanel::GitLog)
        && app.show_right_panel
    {
        vec![
            hint_key("^G"),
            hint_label(" back "),
            hint_key("^X"),
            hint_label(" exit "),
        ]
    } else {
        vec![
            hint_key("^P"),
            hint_label(" commands "),
            hint_key("^B"),
            hint_label(" files "),
            hint_key("^G"),
            hint_label(" git "),
            hint_key("^X"),
            hint_label(" exit "),
        ]
    };

    let git_w: usize = git_spans.iter().map(|s| s.content.width()).sum();
    let job_w: usize = job_spans.iter().map(|s| s.content.width()).sum();
    let hints_w: usize = hint_spans.iter().map(|s| s.content.width()).sum();

    // Truncate path if needed — never truncate branch/changes before path
    let max_path_w = w
        .saturating_sub(git_w)
        .saturating_sub(job_w)
        .saturating_sub(hints_w)
        .saturating_sub(2); // minimal padding
    let path_display = if directory.width() > max_path_w && max_path_w > 4 {
        truncate_str(&directory, max_path_w)
    } else {
        directory.clone()
    };

    let path_span = Span::styled(
        format!(" {} ", path_display),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let path_w = path_display.width() + 2;

    if is_narrow {
        // Two-row layout
        // Row 1: [path] [job?] [pad] [branch + stats]
        let line1_pad = w.saturating_sub(path_w + job_w + git_w);
        let mut line1_spans = vec![path_span.clone()];
        line1_spans.extend(job_spans.iter().cloned());
        line1_spans.push(Span::styled(
            " ".repeat(line1_pad),
            Style::default().bg(Color::Rgb(40, 40, 40)),
        ));
        line1_spans.extend(git_spans.iter().cloned());
        let line1 = Line::from(line1_spans);

        // Row 2: [hints left-aligned]
        let line2_pad = w.saturating_sub(hints_w);
        let mut line2_spans: Vec<Span> = Vec::new();
        line2_spans.push(Span::styled(
            " ",
            Style::default().bg(Color::Rgb(40, 40, 40)),
        ));
        line2_spans.extend(hint_spans.iter().cloned());
        if line2_pad > 1 {
            line2_spans.push(Span::styled(
                " ".repeat(line2_pad.saturating_sub(1)),
                Style::default().bg(Color::Rgb(40, 40, 40)),
            ));
        }
        let line2 = Line::from(line2_spans);

        let text = Text::from(vec![line1, line2]);
        frame.render_widget(Paragraph::new(text), area);
    } else {
        // Single-row: [path] [branch+stats] [job?] [pad] [hints]
        let left_w = path_w + git_w + job_w;
        let padding = w.saturating_sub(left_w + hints_w);

        let mut spans = vec![path_span];
        spans.extend(git_spans);
        spans.extend(job_spans);
        spans.push(Span::styled(
            " ".repeat(padding),
            Style::default().bg(Color::Rgb(40, 40, 40)),
        ));
        spans.extend(hint_spans);

        let bar = Line::from(spans);
        frame.render_widget(Paragraph::new(bar), area);
    }
}

fn hint_key(key: &str) -> Span<'_> {
    Span::styled(
        key,
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(40, 40, 40))
            .add_modifier(Modifier::BOLD),
    )
}

fn hint_label(label: &str) -> Span<'_> {
    Span::styled(
        label,
        Style::default()
            .fg(Color::DarkGray)
            .bg(Color::Rgb(40, 40, 40)),
    )
}

/// Dim every cell in `area` to simulate `rgba(0,0,0,0.5)` placed over the content.
/// Reads each already-rendered cell's fg/bg from the frame buffer and blends it
/// 50% toward black.  The popup renders on top afterwards, so only the content
/// *behind* the popup is affected.
fn draw_backdrop(frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut(Position { x, y }) {
                cell.fg = blend_black(named_to_rgb_fg(cell.fg));
                cell.bg = blend_black(named_to_rgb_bg(cell.bg));
            }
        }
    }
}

/// Convert a foreground Color to its closest Rgb approximation.
/// `Reset` fg is treated as the typical terminal default foreground (near-white).
fn named_to_rgb_fg(c: Color) -> Color {
    match c {
        Color::Reset        => Color::Rgb(204, 204, 204),
        Color::Black        => Color::Rgb(0,   0,   0  ),
        Color::Red          => Color::Rgb(170, 0,   0  ),
        Color::Green        => Color::Rgb(0,   170, 0  ),
        Color::Yellow       => Color::Rgb(170, 170, 0  ),
        Color::Blue         => Color::Rgb(0,   0,   170),
        Color::Magenta      => Color::Rgb(170, 0,   170),
        Color::Cyan         => Color::Rgb(0,   170, 170),
        Color::Gray         => Color::Rgb(170, 170, 170),
        Color::DarkGray     => Color::Rgb(85,  85,  85 ),
        Color::LightRed     => Color::Rgb(255, 85,  85 ),
        Color::LightGreen   => Color::Rgb(85,  255, 85 ),
        Color::LightYellow  => Color::Rgb(255, 255, 85 ),
        Color::LightBlue    => Color::Rgb(85,  85,  255),
        Color::LightMagenta => Color::Rgb(255, 85,  255),
        Color::LightCyan    => Color::Rgb(85,  255, 255),
        Color::White        => Color::Rgb(255, 255, 255),
        other               => other,
    }
}

/// Convert a background Color to its closest Rgb approximation.
/// `Reset` bg is treated as the typical terminal default background (near-black).
fn named_to_rgb_bg(c: Color) -> Color {
    match c {
        Color::Reset => Color::Rgb(18, 18, 18),
        other        => named_to_rgb_fg(other),
    }
}

/// Blend an Rgb color 50% toward black: `result = src * 0.5 + black * 0.5`.
/// Non-Rgb values pass through unchanged (shouldn't normally occur after
/// the named_to_rgb_* calls above).
fn blend_black(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r / 2, g / 2, b / 2),
        other               => other,
    }
}

fn draw_confirm_dialog(frame: &mut Frame, area: Rect) {
    let dialog_width = 40u16;
    let dialog_height = 5u16;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(ratatui::widgets::Clear, dialog_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Close this session? (y/n) ",
            Style::default().fg(Color::Yellow),
        )),
    ];

    let paragraph = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(FOCUSED_BORDER))
            .title(Span::styled(
                " Confirm ",
                Style::default()
                    .fg(FOCUSED_BORDER)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black).fg(Color::White)),
    );

    frame.render_widget(paragraph, dialog_area);
}

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect) {
    const ROWS: usize = 5;
    let dialog_width = 70u16.min(area.width.saturating_sub(4));
    let dialog_height = (ROWS as u16 + 6).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FOCUSED_BORDER))
        .title(Span::styled(
            " Settings ",
            Style::default().fg(FOCUSED_BORDER).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let label_w = 18u16;
    let value_w = inner.width.saturating_sub(label_w + 4);

    // Theme display name lookup
    let theme_display = crate::app::App::EDITOR_THEMES
        .iter()
        .find(|(id, _)| *id == app.config.editor_theme.as_str())
        .map(|(_, name)| *name)
        .unwrap_or("Unknown");

    // Rows: label + value string (theme row handled specially below)
    let field_values: [(&str, &str); ROWS] = [
        ("Shell Command",  &app.config.command),
        ("Editor Command", &app.config.editor_command),
        ("Projects Dir",   &app.config.projects_dir),
        ("Icons",          if app.config.icons { "on" } else { "off" }),
        ("Editor Theme",   theme_display),
    ];

    let dim = Style::default().fg(Color::Rgb(100, 100, 100));
    let active_bg = Color::Rgb(40, 40, 60);

    for (i, (label, value)) in field_values.iter().enumerate() {
        let row_y = inner.y + 1 + i as u16;
        if row_y >= inner.y + inner.height { break; }

        let is_selected = i == app.settings_row;
        let row_bg = if is_selected { active_bg } else { Color::Rgb(20, 20, 30) };
        let label_style = if is_selected {
            Style::default().fg(Color::Cyan).bg(row_bg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(160, 160, 160)).bg(row_bg)
        };

        let indicator = if is_selected { "►" } else { " " };
        let indicator_span = Span::styled(indicator, label_style);
        let label_text = format!("{:<width$}", label, width = label_w as usize);
        let label_span = Span::styled(label_text, label_style);

        let row_area = Rect::new(inner.x, row_y, inner.width, 1);

        if i == 4 {
            // Theme row: show ◀ name ▶ selector
            let theme_names: Vec<&str> = crate::app::App::EDITOR_THEMES
                .iter().map(|(_, n)| *n).collect();
            let cur_idx = crate::app::App::EDITOR_THEMES
                .iter()
                .position(|(id, _)| *id == app.config.editor_theme.as_str())
                .unwrap_or(0);
            let prev_name = theme_names[(cur_idx + theme_names.len() - 1) % theme_names.len()];
            let next_name = theme_names[(cur_idx + 1) % theme_names.len()];

            let (arrow_style, name_style, hint_style) = if is_selected {
                (
                    Style::default().fg(Color::Cyan).bg(row_bg),
                    Style::default().fg(Color::White).bg(row_bg).add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::Rgb(120, 120, 140)).bg(row_bg),
                )
            } else {
                (
                    Style::default().fg(Color::Rgb(80, 80, 100)).bg(row_bg),
                    Style::default().fg(Color::Rgb(210, 210, 210)).bg(row_bg),
                    Style::default().fg(Color::Rgb(80, 80, 100)).bg(row_bg),
                )
            };

            let prev_hint = format!(" {} ", prev_name);
            let next_hint = format!(" {} ", next_name);
            let value_str = format!(" {} ", value);

            let spans = vec![
                indicator_span,
                label_span,
                Span::styled("◀", arrow_style),
                Span::styled(prev_hint, hint_style),
                Span::styled("│", Style::default().fg(Color::Rgb(60, 60, 80)).bg(row_bg)),
                Span::styled(value_str, name_style),
                Span::styled("│", Style::default().fg(Color::Rgb(60, 60, 80)).bg(row_bg)),
                Span::styled(next_hint, hint_style),
                Span::styled("▶", arrow_style),
            ];
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(row_bg)),
                row_area,
            );
        } else {
            // Standard row
            let value_str = if is_selected && app.settings_editing {
                format!("{}_", app.settings_buf)
            } else {
                value.to_string()
            };
            let truncated = if value_str.len() > value_w as usize {
                format!("…{}", &value_str[value_str.len().saturating_sub(value_w as usize - 1)..])
            } else {
                value_str
            };
            let value_style = if is_selected {
                Style::default().fg(Color::White).bg(row_bg)
            } else {
                Style::default().fg(Color::Rgb(210, 210, 210)).bg(row_bg)
            };
            let value_span = Span::styled(truncated, value_style);
            frame.render_widget(
                Paragraph::new(Line::from(vec![indicator_span, label_span, value_span]))
                    .style(Style::default().bg(row_bg)),
                row_area,
            );
        }
    }

    // Hint line at bottom
    let hint_y = inner.y + inner.height.saturating_sub(1);
    if hint_y > inner.y {
        let hint = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Cyan)),
            Span::styled(": nav  ", dim),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(": edit/toggle  ", dim),
            Span::styled("◀▶", Style::default().fg(Color::Cyan)),
            Span::styled(": theme  ", dim),
            Span::styled("^S", Style::default().fg(Color::Green)),
            Span::styled(": save  ", dim),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(": cancel", dim),
        ]);
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().bg(Color::Rgb(20, 20, 30))),
            Rect::new(inner.x, hint_y, inner.width, 1),
        );
    }
}

fn draw_command_palette(frame: &mut Frame, app: &mut App, area: Rect) {
    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let dialog_height = 20u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.height.min(3); // Position near top like VS Code
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear underlying cells so nothing bleeds through on close
    frame.render_widget(ratatui::widgets::Clear, dialog_area);

    let inner_height = dialog_height.saturating_sub(2) as usize;

    let palette_items = if app.show_command_palette {
        app.palette_items_cached()
    } else {
        let filtered = app.filtered_projects();
        filtered
            .iter()
            .map(|p| crate::app::PaletteItem {
                label: p.clone(),
                subtitle: String::new(),
                kind: crate::app::PaletteKind::OpenProject(p.clone()),
            })
            .collect()
    };
    let filter = if app.show_command_palette {
        &app.command_palette_filter
    } else {
        &app.picker_filter
    };
    let selected = if app.show_command_palette {
        app.command_palette_selected
    } else {
        app.picker_selected
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(" > ", Style::default().fg(Color::Cyan)),
        Span::styled(filter, Style::default().fg(Color::White)),
        Span::styled(
            "_",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ])];

    let inner_w = dialog_width.saturating_sub(2) as usize;
    let visible_slots = inner_height.saturating_sub(1);
    let scroll_start = if selected >= visible_slots {
        selected - visible_slots + 1
    } else {
        0
    };

    for (i, item) in palette_items
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_slots)
    {
        let is_sel = i == selected;
        let bg = if is_sel {
            Color::Rgb(60, 60, 80)
        } else {
            Color::Rgb(30, 30, 30)
        };

        let kind_str = match &item.kind {
            crate::app::PaletteKind::OpenFolder => "folder",
            crate::app::PaletteKind::OpenProject(_) => "project",
            crate::app::PaletteKind::NewTerminal => "command",
            crate::app::PaletteKind::ToggleGit => "command",
            crate::app::PaletteKind::ToggleFileBrowser => "command",
            crate::app::PaletteKind::OpenSettings => "command",
            crate::app::PaletteKind::ProjectFile(_) => "file",
            crate::app::PaletteKind::RunCommand(_) => "git",
            crate::app::PaletteKind::GitCheckout(_) => "branch",
        };

        // For file items, prefix the label with a Nerd Font icon when icons enabled
        let is_file_item = matches!(&item.kind, crate::app::PaletteKind::ProjectFile(_));
        let icon_prefix = if app.icons && is_file_item {
            nf_entry_icon(&item.label, false, false)
        } else {
            ""
        };
        let icon_color = if app.icons && is_file_item {
            nf_entry_icon_color(&item.label, false)
        } else {
            Color::Rgb(200, 200, 200)
        };

        let prefix = if is_sel { " > " } else { "   " };
        let prefix_w = prefix.width();
        // +1 for the extra space added when rendering
        let icon_w = if icon_prefix.is_empty() { 0 } else { icon_prefix.trim_end().width() + 2 };
        let label_w = item.label.width();
        let kind_w = kind_str.width();

        let mut spans = Vec::new();
        spans.push(Span::styled(
            prefix,
            Style::default().fg(Color::Cyan).bg(bg),
        ));
        if !icon_prefix.is_empty() {
            // icon already has one trailing space; add second for breathing room
            let icon_display = format!("{} ", icon_prefix.trim_end());
            spans.push(Span::styled(icon_display, Style::default().fg(icon_color).bg(bg)));
        }
        let label_fg = if is_sel { Color::White } else { Color::Rgb(200, 200, 200) };
        let label_modifier = if is_sel { Modifier::BOLD } else { Modifier::empty() };
        spans.push(Span::styled(
            &item.label,
            Style::default().fg(label_fg).bg(bg).add_modifier(label_modifier),
        ));

        // Show subtitle (file path) in dim
        let mut used = prefix_w + icon_w + label_w;
        if !item.subtitle.is_empty() {
            let sub = format!("  {}", item.subtitle);
            let sub_w = sub.width();
            // Truncate subtitle if needed to leave room for kind
            let avail = inner_w.saturating_sub(used + kind_w + 2);
            if avail > 4 {
                let display = truncate_str(&sub, avail);
                used += display.width();
                spans.push(Span::styled(
                    display,
                    Style::default().fg(Color::Rgb(100, 100, 100)).bg(bg),
                ));
            } else {
                let _ = sub_w; // not enough room
            }
        }

        // Right-align the kind label
        let pad = inner_w.saturating_sub(used + kind_w);
        if pad > 0 {
            spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
        }
        spans.push(Span::styled(
            kind_str,
            Style::default().fg(Color::DarkGray).bg(bg),
        ));

        // Fill any remaining space
        let final_w: usize = spans.iter().map(|s| s.content.width()).sum();
        let end_pad = inner_w.saturating_sub(final_w);
        if end_pad > 0 {
            spans.push(Span::styled(" ".repeat(end_pad), Style::default().bg(bg)));
        }

        lines.push(Line::from(spans));
    }

    let title = if app.show_command_palette {
        " Command Palette "
    } else {
        " Open Folder "
    };

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(80, 80, 120)))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Rgb(30, 30, 30)).fg(Color::White)),
    );

    frame.render_widget(paragraph, dialog_area);
}

fn draw_file_browser(frame: &mut Frame, app: &App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::FileBrowser;

    let block = if is_narrow {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 60)))
            .style(Style::default().bg(Color::Rgb(25, 25, 25)))
    } else {
        focused_block(" Explorer ", is_focused)
    };

    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(3) as usize; // borders + scrollbar
    let mut lines: Vec<Line> = Vec::new();

    if app.file_browser.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            " No files",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let scroll = app.file_browser.scroll_offset as usize;

        for (i, entry) in app
            .file_browser
            .entries
            .iter()
            .enumerate()
            .skip(scroll)
            .take(inner_height)
        {
            let is_sel = i == app.file_browser.selected;
            let is_open = app
                .viewing_file
                .as_ref()
                .map(|p| entry.path == std::path::Path::new(p))
                .unwrap_or(false);
            let indent = "  ".repeat(entry.depth);
            let icon = if app.icons {
                nf_entry_icon(&entry.name, entry.is_dir, entry.expanded)
            } else {
                ascii_entry_icon(entry.is_dir, entry.expanded)
            };
            let icon_color = if app.icons {
                nf_entry_icon_color(&entry.name, entry.is_dir)
            } else {
                Color::Rgb(100, 100, 100)
            };

            let name_color = if entry.is_ignored {
                Color::Rgb(100, 100, 100)
            } else {
                match entry.git_status {
                    Some('A') => Color::Green,
                    Some('M') => Color::Yellow,
                    Some('D') => Color::Red,
                    Some('?') => Color::DarkGray,
                    _ => {
                        if entry.is_dir {
                            Color::Rgb(200, 200, 200)
                        } else {
                            Color::Rgb(180, 180, 180)
                        }
                    }
                }
            };

            // Background: open file > selected > transparent
            let row_bg = if is_open {
                Color::Rgb(30, 60, 120)
            } else if is_sel {
                Color::Rgb(55, 55, 85)
            } else {
                Color::Reset
            };

            let has_bg = row_bg != Color::Reset;
            let mut spans = vec![
                // indent prefix (dim)
                Span::styled(
                    format!(" {}", indent),
                    if has_bg {
                        Style::default().fg(Color::Rgb(60, 60, 60)).bg(row_bg)
                    } else {
                        Style::default().fg(Color::Rgb(60, 60, 60))
                    },
                ),
                // icon (brand color) — two trailing spaces for breathing room
                Span::styled(
                    format!("{}  ", icon.trim_end()),
                    if has_bg {
                        Style::default().fg(icon_color).bg(row_bg)
                    } else {
                        Style::default().fg(icon_color)
                    },
                ),
                Span::styled(&entry.name, {
                    let mut mods = if entry.is_dir {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    };
                    if entry.is_symlink {
                        mods |= Modifier::ITALIC;
                    }
                    let mut s = Style::default().fg(name_color).add_modifier(mods);
                    if has_bg {
                        s = s.bg(row_bg);
                    }
                    s
                }),
            ];

            // Calculate used width so far (icon gets two spaces, matching the rendered span)
            let prefix_str = format!(" {}{}  ", indent, icon.trim_end());
            let prefix_w = prefix_str.width();
            let name_w = entry.name.width();

            // Right-side indicators (symlink + git status)
            // Symlink icon "󱞩" (NF link icon) — 2 chars wide + 1 space gap
            let symlink_indicator = "\u{F17A9}"; // 󱞩  nf-md-link_variant
            let symlink_display = if entry.is_symlink { format!(" {}", symlink_indicator) } else { String::new() };
            let symlink_w = symlink_display.width();

            // Git status indicator on the right
            let status_str = match entry.git_status {
                Some('A') => Some(("A", Color::Green)),
                Some('M') => Some(("M", Color::Yellow)),
                Some('D') => Some(("D", Color::Red)),
                Some('?') => Some(("U", Color::DarkGray)),
                _ => None,
            };

            // right_fixed = symlink (if any) + status (if any) + trailing space
            let status_w = if status_str.is_some() { 2usize } else { 0 }; // " X"
            let right_w = symlink_w + status_w;
            let used_w = prefix_w + name_w + right_w;

            // padding between name and right-side indicators
            let pad = inner_width.saturating_sub(used_w);
            spans.push(Span::styled(
                " ".repeat(pad),
                if has_bg { Style::default().bg(row_bg) } else { Style::default() },
            ));

            // Symlink icon, right-aligned before git status
            if entry.is_symlink {
                spans.push(Span::styled(
                    symlink_display,
                    if has_bg {
                        Style::default().fg(Color::Rgb(100, 150, 200)).bg(row_bg)
                    } else {
                        Style::default().fg(Color::Rgb(100, 150, 200))
                    },
                ));
            }

            if let Some((ch, color)) = status_str {
                spans.push(Span::styled(
                    format!(" {}", ch),
                    if has_bg {
                        Style::default().fg(color).bg(row_bg)
                    } else {
                        Style::default().fg(color)
                    },
                ));
            }

            lines.push(Line::from(spans));
        }
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);

    // Scrollbar for file browser
    let total = app.file_browser.entries.len() as u16;
    let visible = inner_height as u16;
    let max_scroll = total.saturating_sub(visible);
    if max_scroll > 0 {
        render_scrollbar(
            frame,
            area,
            is_narrow,
            app.file_browser.scroll_offset,
            max_scroll,
            is_focused,
        );
    }
}

/// Render a horizontal scrollbar along the bottom edge of a panel area.
fn render_horizontal_scrollbar(
    frame: &mut Frame,
    area: Rect,
    is_narrow: bool,
    scroll_offset: u16,
    max_scroll: u16,
    focused: bool,
) {
    if max_scroll == 0 || area.width < 5 {
        return;
    }

    let border_left: u16 = if is_narrow { 0 } else { 1 };
    let border_right: u16 = if is_narrow { 0 } else { 1 };
    let track_width = area.width.saturating_sub(border_left + border_right).max(1) as usize;
    if track_width < 2 {
        return;
    }

    let total_content = max_scroll as usize + track_width;
    let thumb_size = ((track_width as f64 * track_width as f64) / total_content as f64)
        .ceil()
        .max(1.0)
        .min(track_width as f64) as usize;

    let scrollable = track_width.saturating_sub(thumb_size);
    let thumb_pos = if max_scroll > 0 {
        ((scroll_offset as f64 / max_scroll as f64) * scrollable as f64).round() as usize
    } else {
        0
    };

    let thumb_color = if focused {
        SCROLLBAR_THUMB_FOCUSED
    } else {
        SCROLLBAR_THUMB_UNFOCUSED
    };

    let bar_y = area.y + area.height.saturating_sub(1);
    let bar_x_start = area.x + border_left;

    let buf = frame.buffer_mut();
    for i in 0..track_width {
        let x = bar_x_start + i as u16;
        if x >= area.x + area.width.saturating_sub(border_right) {
            break;
        }
        let is_thumb = i >= thumb_pos && i < thumb_pos + thumb_size;
        let ch = if is_thumb { "━" } else { "─" };
        let style = if is_thumb {
            Style::default().fg(thumb_color)
        } else {
            Style::default().fg(SCROLLBAR_TRACK)
        };
        if let Some(cell) = buf.cell_mut((x, bar_y)) {
            cell.set_symbol(ch);
            cell.set_style(style);
        }
    }
}

fn draw_file_viewer(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    use ratatui::text::Line as RatatuiLine;
    let is_focused = app.focus == FocusPanel::FileViewer;
    let file_path = app.viewing_file.as_deref().unwrap_or("");

    let title_line: RatatuiLine = if !file_path.is_empty() {
        let name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        let title_color = if is_focused { FOCUSED_BORDER } else { UNFOCUSED_BORDER };
        let title_modifier = if is_focused { Modifier::BOLD } else { Modifier::empty() };
        if app.icons {
            let icon = nf_entry_icon(name, false, false);
            let icon_color = nf_entry_icon_color(name, false);
            RatatuiLine::from(vec![
                Span::styled(" ", Style::default().fg(title_color)),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::styled(
                    format!("{} ", name),
                    Style::default().fg(title_color).add_modifier(title_modifier),
                ),
            ])
        } else {
            RatatuiLine::from(Span::styled(
                format!(" {} ", name),
                Style::default().fg(title_color).add_modifier(title_modifier),
            ))
        }
    } else {
        let title_color = if is_focused { FOCUSED_BORDER } else { UNFOCUSED_BORDER };
        RatatuiLine::from(Span::styled(" File ", Style::default().fg(title_color)))
    };

    let block = if is_narrow {
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 60)))
            .title(title_line)
    } else {
        let border_color = if is_focused { FOCUSED_BORDER } else { UNFOCUSED_BORDER };
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(title_line)
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Read scroll state reported by aide-editor (parsed from OSC title in EditorPane::drain)
    let (editor_scroll, editor_total, editor_view_h, editor_scroll_col, editor_max_col) =
        if let Some(ep) = &app.editor_pane {
            (ep.editor_scroll, ep.editor_total, ep.editor_view_h,
             ep.editor_scroll_col, ep.editor_max_col)
        } else {
            (0u64, 1u64, inner.height as u64, 0u64, 0u64)
        };

    let show_scrollbar = app.editor_pane.is_some() && editor_total > editor_view_h;
    // Reserve 1 column on the right for the scrollbar when it will be shown
    let scrollbar_w: u16 = if show_scrollbar { 1 } else { 0 };
    let content_area = Rect::new(inner.x, inner.y, inner.width.saturating_sub(scrollbar_w), inner.height);

    // Store dimensions and content area so the event loop knows where to forward clicks
    app.editor_pane_rows = content_area.height;
    app.editor_pane_cols = content_area.width;
    app.file_viewer_content_area = content_area;

    if let Some(ep) = &mut app.editor_pane {
        // Keep parser in sync with the viewport size
        {
            let screen = ep.parser.screen_mut();
            let (cur_rows, cur_cols) = screen.size();
            if cur_rows != content_area.height || cur_cols != content_area.width {
                screen.set_size(content_area.height, content_area.width);
            }
        }
        let text = vt100_screen_to_text(ep.parser.screen_mut(), None);
        frame.render_widget(Paragraph::new(text), content_area);

        // Forward the cursor position to ratatui so it appears in the right spot
        let screen = ep.parser.screen_mut();
        if !screen.hide_cursor() {
            let (crow, ccol) = screen.cursor_position();
            let cx = content_area.x + ccol;
            let cy = content_area.y + crow;
            if cx < content_area.x + content_area.width && cy < content_area.y + content_area.height {
                let buf = frame.buffer_mut();
                if let Some(cell) = buf.cell_mut((cx, cy)) {
                    if is_focused {
                        if cell.modifier.contains(Modifier::REVERSED) {
                            cell.modifier.remove(Modifier::REVERSED);
                        } else {
                            cell.modifier.insert(Modifier::REVERSED);
                        }
                    } else {
                        cell.set_fg(Color::DarkGray);
                        cell.set_bg(Color::DarkGray);
                    }
                }
            }
        }

        // Vertical scrollbar on the right edge
        if show_scrollbar {
            let max_scroll = editor_total.saturating_sub(editor_view_h) as u16;
            render_scrollbar(frame, area, is_narrow, editor_scroll as u16, max_scroll, is_focused);
        }

        // Horizontal scrollbar overlaid on the bottom border
        const GUTTER: u64 = 5;
        let h_visible = content_area.width.saturating_sub(GUTTER as u16) as u64;
        let h_max_scroll = editor_max_col.saturating_sub(h_visible);
        if h_max_scroll > 0 {
            render_horizontal_scrollbar(
                frame, area, is_narrow,
                editor_scroll_col as u16, h_max_scroll as u16, is_focused,
            );
        }
    } else {
        let placeholder = Paragraph::new(" No file open — use the file browser or Ctrl+P to open a file")
            .style(Style::default().fg(Color::Rgb(80, 80, 80)));
        frame.render_widget(placeholder, content_area);
    }
}


