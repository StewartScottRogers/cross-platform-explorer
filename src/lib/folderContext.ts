/**
 * Folder-context plugin registry (CPE-235, expanded to ~100 recognizers in
 * CPE-239). Mirrors the file-type provider idea one level up: each recognizer
 * cheaply inspects a folder's LISTING for marker files/dirs (never a deep scan)
 * and, if matched, claims a context with a label, icon, and a one-click action
 * to open the folder's content in the appropriate external application.
 *
 * A folder may match several recognizers; results aggregate. "Open in <app>" is
 * `openPath` on the marker (or the folder), which the OS routes to the associated
 * app (.sln → Visual Studio, .uproject → Unreal, …); where no app is registered
 * it falls back to the default handler. Git is special-cased to open its remote
 * on GitHub.
 *
 * Extensible by design: add a row to RECOGNIZERS. Detection is data-driven, so
 * new folder types are one line, not new code.
 */
import type { DirEntry } from "./types";

export type FolderActionKind = "open-path" | "open-github";

export interface FolderAction {
  id: string;
  label: string;
  kind: FolderActionKind;
  /** File/folder path (open-path) or repo folder path (open-github). */
  target: string;
}

export interface FolderContext {
  id: string;
  label: string;
  icon: string;
  detail?: string;
  actions: FolderAction[];
}

export interface FolderProbe {
  path: string;
  entries: DirEntry[];
}

interface Recognizer {
  id: string;
  label: string;
  /** Button text, e.g. "Open in Visual Studio". */
  action: string;
  icon?: string;
  /** Match a file/dir whose (lowercased) name equals one of these. */
  files?: string[];
  /** …or an entry whose (lowercased) name ends with one of these. */
  exts?: string[];
  /** …or a directory whose (lowercased) name equals one of these. */
  dirs?: string[];
  /** Skip this recognizer if any of these (lowercased) names is present. */
  unless?: string[];
  /** Open the matched marker (default) or the folder itself. */
  open?: "marker" | "folder";
  /** open-github reads .git/config for the remote; default open-path. */
  actionKind?: FolderActionKind;
}

const C = "code", WEB = "web", DB = "database", GAME = "cube", DOC = "document", VM = "disk";

// ~100 recognizers. Marker names are compared lowercased.
const RECOGNIZERS: Recognizer[] = [
  // ---- Version control (Git first; special GitHub action) ----
  { id: "git", label: "Git repository", action: "Open on GitHub", dirs: [".git"], open: "folder", actionKind: "open-github", icon: C },
  { id: "hg", label: "Mercurial repository", action: "Reveal .hg", dirs: [".hg"], open: "folder", icon: C },
  { id: "svn", label: "Subversion checkout", action: "Reveal .svn", dirs: [".svn"], open: "folder", icon: C },
  { id: "bzr", label: "Bazaar branch", action: "Reveal .bzr", dirs: [".bzr"], open: "folder", icon: C },

  // ---- IDE / editor projects ----
  { id: "vs-sln", label: "Visual Studio solution", action: "Open in Visual Studio", exts: [".sln"], icon: C },
  { id: "vs-csproj", label: "C# project", action: "Open in Visual Studio", exts: [".csproj"], icon: C },
  { id: "vs-vcxproj", label: "C++ project", action: "Open in Visual Studio", exts: [".vcxproj"], icon: C },
  { id: "vs-fsproj", label: "F# project", action: "Open in Visual Studio", exts: [".fsproj"], icon: C },
  { id: "vs-vbproj", label: "VB.NET project", action: "Open in Visual Studio", exts: [".vbproj"], icon: C },
  { id: "vscode", label: "VS Code workspace", action: "Open in VS Code", exts: [".code-workspace"], icon: C },
  { id: "xcode-proj", label: "Xcode project", action: "Open in Xcode", exts: [".xcodeproj"], icon: C },
  { id: "xcode-ws", label: "Xcode workspace", action: "Open in Xcode", exts: [".xcworkspace"], icon: C },
  { id: "intellij", label: "IntelliJ IDEA project", action: "Open in IntelliJ IDEA", dirs: [".idea"], icon: C },
  { id: "eclipse", label: "Eclipse project", action: "Open in Eclipse", files: [".project"], icon: C },
  { id: "sublime", label: "Sublime Text project", action: "Open in Sublime Text", exts: [".sublime-project"], icon: C },
  { id: "netbeans", label: "NetBeans project", action: "Open in NetBeans", dirs: ["nbproject"], icon: C },
  { id: "qtcreator", label: "Qt project", action: "Open in Qt Creator", exts: [".pro", ".qbs"], icon: C },
  { id: "codeblocks", label: "Code::Blocks project", action: "Open in Code::Blocks", exts: [".cbp"], icon: C },
  { id: "rproj", label: "R project", action: "Open in RStudio", exts: [".rproj"], icon: C },
  { id: "devcontainer", label: "Dev Container", action: "Open devcontainer", dirs: [".devcontainer"], icon: C },

  // ---- Build systems ----
  { id: "cmake", label: "CMake project", action: "Open CMakeLists.txt", files: ["cmakelists.txt"], icon: C },
  { id: "make", label: "Make project", action: "Open Makefile", files: ["makefile", "gnumakefile"], icon: C },
  { id: "gradle", label: "Gradle project", action: "Open build.gradle", files: ["build.gradle", "build.gradle.kts"], icon: C },
  { id: "maven", label: "Maven project", action: "Open pom.xml", files: ["pom.xml"], icon: C },
  { id: "ant", label: "Ant project", action: "Open build.xml", files: ["build.xml"], icon: C },
  { id: "bazel", label: "Bazel workspace", action: "Open WORKSPACE", files: ["workspace", "workspace.bazel"], icon: C },
  { id: "meson", label: "Meson project", action: "Open meson.build", files: ["meson.build"], icon: C },
  { id: "scons", label: "SCons project", action: "Open SConstruct", files: ["sconstruct"], icon: C },
  { id: "ninja", label: "Ninja build", action: "Open build.ninja", files: ["build.ninja"], icon: C },
  { id: "premake", label: "Premake project", action: "Open premake5.lua", files: ["premake5.lua"], icon: C },
  { id: "autotools", label: "Autotools project", action: "Open configure.ac", files: ["configure.ac", "configure.in"], icon: C },
  { id: "buck", label: "Buck project", action: "Open .buckconfig", files: [".buckconfig"], icon: C },

  // ---- Language / package ecosystems ----
  { id: "node", label: "Node.js project", action: "Open package.json", files: ["package.json"], icon: C },
  { id: "deno", label: "Deno project", action: "Open deno.json", files: ["deno.json", "deno.jsonc"], icon: C },
  { id: "rust", label: "Rust crate", action: "Open Cargo.toml", files: ["cargo.toml"], icon: C },
  { id: "python-pyproject", label: "Python project", action: "Open pyproject.toml", files: ["pyproject.toml"], icon: C },
  { id: "python-reqs", label: "Python (requirements)", action: "Open requirements.txt", files: ["requirements.txt"], icon: C },
  { id: "pipenv", label: "Pipenv project", action: "Open Pipfile", files: ["pipfile"], icon: C },
  { id: "conda", label: "Conda environment", action: "Open environment.yml", files: ["environment.yml", "environment.yaml"], icon: C },
  { id: "setuppy", label: "Python (setuptools)", action: "Open setup.py", files: ["setup.py"], icon: C },
  { id: "go", label: "Go module", action: "Open go.mod", files: ["go.mod"], icon: C },
  { id: "ruby", label: "Ruby project", action: "Open Gemfile", files: ["gemfile"], icon: C },
  { id: "gemspec", label: "Ruby gem", action: "Open gemspec", exts: [".gemspec"], icon: C },
  { id: "composer", label: "PHP Composer project", action: "Open composer.json", files: ["composer.json"], icon: C },
  { id: "sbt", label: "Scala sbt project", action: "Open build.sbt", files: ["build.sbt"], icon: C },
  { id: "clojure", label: "Clojure project", action: "Open deps.edn", files: ["deps.edn", "project.clj"], icon: C },
  { id: "elixir", label: "Elixir Mix project", action: "Open mix.exs", files: ["mix.exs"], icon: C },
  { id: "rebar", label: "Erlang project", action: "Open rebar.config", files: ["rebar.config"], icon: C },
  { id: "cabal", label: "Haskell project", action: "Open Cabal file", exts: [".cabal"], files: ["stack.yaml"], icon: C },
  { id: "flutter", label: "Dart / Flutter project", action: "Open pubspec.yaml", files: ["pubspec.yaml"], icon: C },
  { id: "swiftpm", label: "Swift package", action: "Open Package.swift", files: ["package.swift"], icon: C },
  { id: "perl", label: "Perl project", action: "Open Makefile.PL", files: ["makefile.pl", "cpanfile"], icon: C },
  { id: "julia", label: "Julia project", action: "Open Project.toml", files: ["project.toml"], icon: C },
  { id: "nim", label: "Nim project", action: "Open nimble file", exts: [".nimble"], icon: C },
  { id: "crystal", label: "Crystal project", action: "Open shard.yml", files: ["shard.yml"], icon: C },
  { id: "zig", label: "Zig project", action: "Open build.zig", files: ["build.zig"], icon: C },
  { id: "dune", label: "OCaml project", action: "Open dune-project", files: ["dune-project"], icon: C },
  { id: "dub", label: "D project", action: "Open dub.json", files: ["dub.json", "dub.sdl"], icon: C },
  { id: "gleam", label: "Gleam project", action: "Open gleam.toml", files: ["gleam.toml"], icon: C },
  { id: "elm", label: "Elm project", action: "Open elm.json", files: ["elm.json"], icon: C },
  { id: "haxe", label: "Haxe project", action: "Open build.hxml", exts: [".hxml"], icon: C },
  { id: "lua-rocks", label: "Lua project", action: "Open rockspec", exts: [".rockspec"], icon: C },
  { id: "nuget", label: ".NET / NuGet config", action: "Open nuget.config", files: ["nuget.config"], icon: C },

  // ---- JS/TS monorepo & tooling ----
  { id: "pnpm-ws", label: "pnpm workspace", action: "Open pnpm-workspace.yaml", files: ["pnpm-workspace.yaml"], icon: C },
  { id: "lerna", label: "Lerna monorepo", action: "Open lerna.json", files: ["lerna.json"], icon: C },
  { id: "nx", label: "Nx workspace", action: "Open nx.json", files: ["nx.json"], icon: C },
  { id: "turbo", label: "Turborepo", action: "Open turbo.json", files: ["turbo.json"], icon: C },
  { id: "rush", label: "Rush monorepo", action: "Open rush.json", files: ["rush.json"], icon: C },
  { id: "storybook", label: "Storybook", action: "Open .storybook", dirs: [".storybook"], icon: C },

  // ---- Web frameworks / static sites ----
  { id: "web-static", label: "Web page", action: "Open in browser", files: ["index.html"], unless: ["package.json"], open: "marker", icon: WEB },
  { id: "angular", label: "Angular app", action: "Open angular.json", files: ["angular.json"], icon: WEB },
  { id: "vue", label: "Vue app", action: "Open vue.config.js", files: ["vue.config.js"], icon: WEB },
  { id: "svelte", label: "Svelte app", action: "Open svelte.config.js", files: ["svelte.config.js"], icon: WEB },
  { id: "next", label: "Next.js app", action: "Open next.config", files: ["next.config.js", "next.config.mjs", "next.config.ts"], icon: WEB },
  { id: "nuxt", label: "Nuxt app", action: "Open nuxt.config", files: ["nuxt.config.ts", "nuxt.config.js"], icon: WEB },
  { id: "vite", label: "Vite app", action: "Open vite.config", files: ["vite.config.ts", "vite.config.js"], icon: WEB },
  { id: "webpack", label: "webpack project", action: "Open webpack.config.js", files: ["webpack.config.js"], icon: WEB },
  { id: "gatsby", label: "Gatsby site", action: "Open gatsby-config.js", files: ["gatsby-config.js"], icon: WEB },
  { id: "astro", label: "Astro site", action: "Open astro.config", files: ["astro.config.mjs", "astro.config.ts"], icon: WEB },
  { id: "remix", label: "Remix app", action: "Open remix.config.js", files: ["remix.config.js"], icon: WEB },
  { id: "tailwind", label: "Tailwind project", action: "Open tailwind.config", files: ["tailwind.config.js", "tailwind.config.ts", "tailwind.config.cjs"], icon: WEB },

  // ---- Static site / docs generators ----
  { id: "jekyll", label: "Jekyll site", action: "Open _config.yml", files: ["_config.yml"], icon: WEB },
  { id: "hugo", label: "Hugo site", action: "Open hugo config", files: ["hugo.toml", "hugo.yaml"], icon: WEB },
  { id: "mkdocs", label: "MkDocs site", action: "Open mkdocs.yml", files: ["mkdocs.yml"], icon: WEB },
  { id: "docusaurus", label: "Docusaurus site", action: "Open docusaurus.config.js", files: ["docusaurus.config.js"], icon: WEB },
  { id: "sphinx", label: "Sphinx docs", action: "Open conf.py", files: ["conf.py"], icon: DOC },
  { id: "quarto", label: "Quarto project", action: "Open _quarto.yml", files: ["_quarto.yml"], icon: DOC },
  { id: "latex", label: "LaTeX project", action: "Open in editor", exts: [".tex"], icon: DOC },
  { id: "obsidian", label: "Obsidian vault", action: "Open in Obsidian", dirs: [".obsidian"], icon: DOC },
  { id: "jupyter", label: "Jupyter notebooks", action: "Open notebook", exts: [".ipynb"], icon: C },

  // ---- Game engines ----
  { id: "unity", label: "Unity project", action: "Open in Unity", dirs: ["projectsettings"], icon: GAME },
  { id: "unreal", label: "Unreal Engine project", action: "Open in Unreal Editor", exts: [".uproject"], icon: GAME },
  { id: "godot", label: "Godot project", action: "Open in Godot", files: ["project.godot"], icon: GAME },
  { id: "gamemaker", label: "GameMaker project", action: "Open in GameMaker", exts: [".yyp"], icon: GAME },
  { id: "defold", label: "Defold project", action: "Open in Defold", files: ["game.project"], icon: GAME },
  { id: "love2d", label: "LÖVE game", action: "Open conf.lua", files: ["conf.lua"], icon: GAME },
  { id: "blender", label: "Blender project", action: "Open in Blender", exts: [".blend"], icon: GAME },

  // ---- Containers / DevOps / infra ----
  { id: "docker", label: "Docker image", action: "Open Dockerfile", files: ["dockerfile", "containerfile"], icon: VM },
  { id: "compose", label: "Docker Compose", action: "Open compose file", files: ["docker-compose.yml", "docker-compose.yaml", "compose.yaml", "compose.yml"], icon: VM },
  { id: "terraform", label: "Terraform config", action: "Open in editor", exts: [".tf"], icon: C },
  { id: "vagrant", label: "Vagrant environment", action: "Open Vagrantfile", files: ["vagrantfile"], icon: VM },
  { id: "ansible", label: "Ansible project", action: "Open playbook", files: ["ansible.cfg", "playbook.yml", "site.yml"], icon: C },
  { id: "helm", label: "Helm chart", action: "Open Chart.yaml", files: ["chart.yaml"], icon: C },
  { id: "kustomize", label: "Kubernetes (Kustomize)", action: "Open kustomization.yaml", files: ["kustomization.yaml", "kustomization.yml"], icon: C },
  { id: "pulumi", label: "Pulumi project", action: "Open Pulumi.yaml", files: ["pulumi.yaml"], icon: C },
  { id: "serverless", label: "Serverless project", action: "Open serverless.yml", files: ["serverless.yml", "serverless.yaml"], icon: C },
  { id: "skaffold", label: "Skaffold project", action: "Open skaffold.yaml", files: ["skaffold.yaml"], icon: C },
  { id: "nix", label: "Nix project", action: "Open flake.nix", files: ["flake.nix", "default.nix", "shell.nix"], icon: C },

  // ---- CI ----
  { id: "gh-actions", label: "GitHub Actions", action: "Open workflows", dirs: [".github"], icon: C },
  { id: "gitlab-ci", label: "GitLab CI", action: "Open .gitlab-ci.yml", files: [".gitlab-ci.yml"], icon: C },
  { id: "circleci", label: "CircleCI", action: "Open .circleci", dirs: [".circleci"], icon: C },
  { id: "travis", label: "Travis CI", action: "Open .travis.yml", files: [".travis.yml"], icon: C },
  { id: "jenkins", label: "Jenkins pipeline", action: "Open Jenkinsfile", files: ["jenkinsfile"], icon: C },
  { id: "azure-pipelines", label: "Azure Pipelines", action: "Open azure-pipelines.yml", files: ["azure-pipelines.yml"], icon: C },

  // ---- Mobile ----
  { id: "react-native", label: "React Native app", action: "Open metro.config.js", files: ["metro.config.js"], icon: C },
  { id: "expo", label: "Expo app", action: "Open app config", files: ["app.config.js", "app.config.ts"], icon: C },
  { id: "cocoapods", label: "CocoaPods (iOS)", action: "Open Podfile", files: ["podfile"], icon: C },
  { id: "fastlane", label: "Fastlane", action: "Open fastlane", dirs: ["fastlane"], icon: C },

  // ---- Data / ML / APIs / hardware ----
  { id: "dvc", label: "DVC project", action: "Open .dvc", dirs: [".dvc"], icon: DB },
  { id: "dbt", label: "dbt project", action: "Open dbt_project.yml", files: ["dbt_project.yml"], icon: DB },
  { id: "mlflow", label: "MLflow project", action: "Open MLproject", files: ["mlproject"], icon: DB },
  { id: "protobuf", label: "Protocol Buffers", action: "Open .proto", exts: [".proto"], icon: C },
  { id: "openapi", label: "OpenAPI spec", action: "Open spec", files: ["openapi.yaml", "openapi.json", "swagger.yaml", "swagger.json"], icon: C },
  { id: "graphql", label: "GraphQL project", action: "Open schema", files: ["schema.graphql", "codegen.yml", "codegen.yaml"], icon: C },
  { id: "postman", label: "Postman collection", action: "Open collection", exts: [".postman_collection.json"], icon: C },
  { id: "platformio", label: "PlatformIO project", action: "Open platformio.ini", files: ["platformio.ini"], icon: C },
  { id: "arduino", label: "Arduino sketch", action: "Open sketch", exts: [".ino"], icon: C },
  { id: "kicad", label: "KiCad project", action: "Open in KiCad", exts: [".kicad_pro", ".pro"], icon: C },
  { id: "wordpress", label: "WordPress site", action: "Open wp-config.php", files: ["wp-config.php"], icon: WEB },
];

const iconOf = (r: Recognizer) => r.icon ?? C;

/** Aggregate every context a folder matches (empty for plain folders). */
export function detectContexts(probe: FolderProbe): FolderContext[] {
  const files = new Map<string, DirEntry>();
  const dirs = new Map<string, DirEntry>();
  for (const e of probe.entries) {
    (e.is_dir ? dirs : files).set(e.name.toLowerCase(), e);
  }
  const has = (n: string) => files.has(n) || dirs.has(n);
  const findExt = (exts: string[]): DirEntry | undefined => {
    for (const e of probe.entries) {
      const lc = e.name.toLowerCase();
      if (exts.some((x) => lc.endsWith(x))) return e;
    }
    return undefined;
  };

  const out: FolderContext[] = [];
  for (const r of RECOGNIZERS) {
    if (r.unless && r.unless.some(has)) continue;

    let m: DirEntry | undefined;
    if (r.files) for (const n of r.files) { m = files.get(n) ?? dirs.get(n); if (m) break; }
    if (!m && r.dirs) for (const n of r.dirs) { m = dirs.get(n); if (m) break; }
    if (!m && r.exts) m = findExt(r.exts);
    if (!m) continue;

    const target = r.open === "folder" ? probe.path : m.path;
    out.push({
      id: r.id,
      label: r.label,
      icon: iconOf(r),
      actions: [{ id: r.id, label: r.action, kind: r.actionKind ?? "open-path", target }],
    });
  }
  return out;
}
