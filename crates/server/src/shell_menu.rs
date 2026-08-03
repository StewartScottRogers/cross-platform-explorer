//! Shell context-menu model (CPE-945, epic CPE-712): the pure applicability core for "CPE as a shell
//! citizen". A set of registered menu **verbs** (a label + a command template + which selections they
//! apply to) and a function that, given the current selection, returns the verbs to show. No OS shell
//! registration here (that's per-platform glue); this decides *what* to offer.

/// What kind of selection a verb applies to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "applies", rename_all = "snake_case")]
pub enum AppliesTo {
    /// Only when every selected item is a file.
    Files,
    /// Only when every selected item is a folder.
    Folders,
    /// Any selection (files, folders, or a mix).
    Any,
    /// Only when every selected item is a file with one of these (lower-cased, no-dot) extensions.
    Extensions(Vec<String>),
}

/// A registered context-menu verb.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MenuVerb {
    pub id: String,
    pub label: String,
    /// Command template run over the selection (e.g. `cpe open "{path}"`); expansion is the caller's job.
    pub command: String,
    pub applies: AppliesTo,
}

/// A selected item — just what applicability needs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SelItem {
    pub path: String,
    pub is_dir: bool,
}

impl SelItem {
    fn ext(&self) -> Option<String> {
        let name = self.path.rsplit(['/', '\\']).next().unwrap_or(&self.path);
        name.rfind('.').filter(|&d| d > 0).map(|d| name[d + 1..].to_ascii_lowercase())
    }
}

fn verb_applies(applies: &AppliesTo, sel: &[SelItem]) -> bool {
    if sel.is_empty() {
        return false; // no selection ⇒ no per-item verbs
    }
    match applies {
        AppliesTo::Any => true,
        AppliesTo::Files => sel.iter().all(|s| !s.is_dir),
        AppliesTo::Folders => sel.iter().all(|s| s.is_dir),
        AppliesTo::Extensions(exts) => {
            let want: Vec<String> = exts.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect();
            sel.iter().all(|s| !s.is_dir && s.ext().map(|e| want.contains(&e)).unwrap_or(false))
        }
    }
}

/// The verbs to show for `selection`, preserving the registration order. A verb shows only when it
/// applies to **every** selected item (so it's always meaningful for the whole selection).
pub fn verbs_for<'a>(verbs: &'a [MenuVerb], selection: &[SelItem]) -> Vec<&'a MenuVerb> {
    verbs.iter().filter(|v| verb_applies(&v.applies, selection)).collect()
}

// ── Windows shell registration plan (CPE-1019, epic CPE-712) ────────────────────────────────────────
// Turn the "Open in Cross-Platform Explorer" integration into an explicit list of registry operations, as
// *data*, so the apply glue (CPE-1020) stays trivial and the reversibility guarantee is unit-testable. No
// registry I/O here. Everything registers under HKCU\Software\Classes so no elevation is required.

/// One registry value to write when installing the Windows shell integration. `key` is the path **under
/// HKCU**; an empty `value_name` denotes the key's `(Default)` value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegEntry {
    pub key: String,
    pub value_name: String,
    pub value: String,
}

/// The Windows shell-registration plan: values to write on install, and the full key paths to delete on
/// uninstall (deleting a `…\shell\CPE` root removes its `command` subkey with it).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WinShellPlan {
    pub install: Vec<RegEntry>,
    pub remove: Vec<String>,
}

/// Build the HKCU registry plan for the "Open in CPE" context-menu integration.
///
/// Registers three file-explorer surfaces — on a folder, on the folder background (empty space in a
/// directory), and on a drive — each as `Software\Classes\<class>\shell\CPE` with a label + `Icon`, and a
/// `command` subkey invoking `exe_path` with the selected path. Folders/drives pass `"%1"`; the folder
/// background passes `"%V"` (the current directory). `exe_path` is quoted so spaces are safe.
pub fn windows_shell_plan(exe_path: &str, app_name: &str) -> WinShellPlan {
    // (registry class, path placeholder the shell substitutes).
    let surfaces = [("Directory", "%1"), (r"Directory\Background", "%V"), ("Drive", "%1")];
    let label = format!("Open in {app_name}");

    let mut install = Vec::new();
    let mut remove = Vec::new();
    for (class, placeholder) in surfaces {
        let root = format!(r"Software\Classes\{class}\shell\CPE");
        // Label (Default value of the verb key) + the icon shown beside it.
        install.push(RegEntry { key: root.clone(), value_name: String::new(), value: label.clone() });
        install.push(RegEntry { key: root.clone(), value_name: "Icon".into(), value: exe_path.to_string() });
        // The command run when the item is chosen.
        install.push(RegEntry {
            key: format!(r"{root}\command"),
            value_name: String::new(),
            value: format!(r#""{exe_path}" "{placeholder}""#),
        });
        remove.push(root);
    }
    WinShellPlan { install, remove }
}

// ── Linux .desktop + xdg-mime registration plan (CPE-1021, epic CPE-712) ────────────────────────────
// The Linux analogue of `windows_shell_plan`, emitted as data: the `.desktop` entry to install under
// `~/.local/share/applications` (user scope, no root) and the `inode/directory` default association to set
// via `xdg-mime`, plus the files to remove on uninstall. No filesystem or `xdg-mime` process calls here.

/// The Linux shell-registration plan (pure). `home` is passed in (not read from the env) so it's testable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LinuxShellPlan {
    /// Absolute path of the `.desktop` file to write.
    pub desktop_path: String,
    /// Full contents of that `.desktop` file.
    pub desktop_contents: String,
    /// The desktop-file id (basename) `xdg-mime default <id> <mime>` registers.
    pub desktop_id: String,
    /// The mimetype whose default handler this sets (so uninstall knows which association it owns).
    pub mime_type: String,
    /// Filesystem paths to delete on uninstall.
    pub remove_paths: Vec<String>,
}

/// Build the Linux "Open in CPE" registration plan: a `.desktop` launcher that opens folders (`%F`,
/// `MimeType=inode/directory`) with an "Open in <app>" desktop action, installed in the user's
/// applications dir and set as the default `inode/directory` handler.
pub fn linux_shell_plan(exe_path: &str, app_name: &str, home: &str) -> LinuxShellPlan {
    let desktop_id = "cross-platform-explorer.desktop".to_string();
    let desktop_path = format!("{home}/.local/share/applications/{desktop_id}");
    let desktop_contents = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={app_name}\n\
         Exec={exe_path} %F\n\
         Icon={exe_path}\n\
         Terminal=false\n\
         NoDisplay=false\n\
         MimeType=inode/directory;\n\
         Categories=Utility;System;FileManager;\n\
         Actions=OpenInCPE;\n\
         \n\
         [Desktop Action OpenInCPE]\n\
         Name=Open in {app_name}\n\
         Exec={exe_path} %F\n"
    );
    LinuxShellPlan {
        remove_paths: vec![desktop_path.clone()],
        desktop_path,
        desktop_contents,
        desktop_id,
        mime_type: "inode/directory".to_string(),
    }
}

// ── macOS Services / LSHandlers registration plan (CPE-1022, epic CPE-712) ──────────────────────────
// macOS integration is largely declared in the app bundle's Info.plist (Services are read from it), so the
// "plan" here is the plist fragments to include: an `NSServices` entry that puts "Open in <app>" in the
// Services menu for file/folder selections, plus an optional `LSHandlerRoleAll` folder-viewer association.
// Pure/data only — writing the bundle plist + `defaults`/`lsregister` glue (and real-macOS verification)
// is a follow-up slice that needs a Mac.

/// The macOS shell-registration plan (pure): plist fragments to add + the keys to strip on uninstall.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MacShellPlan {
    /// The `NSServices` array-entry `<dict>` (XML plist fragment) for the "Open in <app>" Services item.
    pub ns_service: String,
    /// The `LSHandlers` association `<dict>` making the app a folder ("public.folder") viewer handler.
    pub ls_handler: String,
    /// Identifiers of the fragments to remove on uninstall (the service menu title + the handler content type).
    pub remove_keys: Vec<String>,
}

/// Build the macOS "Open in CPE" plan: an `NSServices` entry (menu title + folder/file send-types +
/// handler method) and an `LSHandlers` folder-viewer association, keyed by `bundle_id`.
pub fn macos_shell_plan(app_name: &str, bundle_id: &str) -> MacShellPlan {
    let menu_title = format!("Open in {app_name}");
    let ns_service = format!(
        "<dict>\n\
         \t<key>NSMenuItem</key>\n\
         \t<dict><key>default</key><string>{menu_title}</string></dict>\n\
         \t<key>NSMessage</key><string>openSelection</string>\n\
         \t<key>NSPortName</key><string>{app_name}</string>\n\
         \t<key>NSSendFileTypes</key>\n\
         \t<array><string>public.folder</string><string>public.item</string></array>\n\
         </dict>\n"
    );
    let ls_handler = format!(
        "<dict>\n\
         \t<key>LSHandlerContentType</key><string>public.folder</string>\n\
         \t<key>LSHandlerRoleAll</key><string>{bundle_id}</string>\n\
         </dict>\n"
    );
    MacShellPlan {
        ns_service,
        ls_handler,
        remove_keys: vec![menu_title, "public.folder".to_string()],
    }
}

// ── Windows "Default apps" registration plan (CPE-1277, epic CPE-712) ────────────────────────────────
// Modern Windows never lets a program silently *become* the default handler — the user must confirm the
// choice in Settings → Default apps. All we can honestly do is REGISTER Cross-Platform Explorer as a
// *candidate*: publish an app-Capabilities entry under HKCU\Software\RegisteredApplications (so CPE shows
// up in the Default-apps app list) plus an "open a folder" ProgID with a launch command. This plan is
// that registration, as data, so the applier stays trivial and the reversal is unit-testable. No registry
// I/O here. Everything lives under HKCU, so no elevation is required.

/// A single registry *value* (its key path under HKCU + the value name) to delete on uninstall — distinct
/// from deleting a whole key subtree, used to drop the `RegisteredApplications` pointer without disturbing
/// the other apps registered alongside it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegValueRef {
    pub key: String,
    pub value_name: String,
}

/// The Windows Default-apps registration plan: values to write on install, key subtrees to delete on
/// uninstall, and individual values to delete on uninstall (the `RegisteredApplications` pointer, whose
/// siblings must survive). Together `remove_keys` + `remove_values` reverse the whole `install` set.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WinDefaultAppsPlan {
    pub install: Vec<RegEntry>,
    pub remove_keys: Vec<String>,
    pub remove_values: Vec<RegValueRef>,
}

/// Stable HKCU vendor key holding our published capabilities. Name-independent, so `default_apps_registered`
/// and the key-removal set need no arguments; the whole subtree is deleted on uninstall.
const CAPS_ROOT: &str = r"Software\CrossPlatformExplorer";
/// ProgID registered as the "open a folder" handler CPE advertises in its capabilities.
const FOLDER_PROGID: &str = "CrossPlatformExplorer.Folder";

/// Build the HKCU plan that registers CPE as a Windows Default-apps *candidate* for opening folders.
///
/// Writes: a `RegisteredApplications` pointer (value name = `app_name`, data = the Capabilities key path)
/// so CPE appears in the Default-apps app list; a `Capabilities` key carrying `ApplicationName` /
/// `ApplicationDescription` and a `FileAssociations` entry that *advertises* the folder ProgID (declares
/// capability only — it never sets the default); and that ProgID itself (`(Default)` label, `DefaultIcon`,
/// and `shell\open\command` → `"exe" "%1"`). This only makes CPE *selectable* in Settings → Default apps;
/// the user still confirms the choice there. `exe_path` is quoted so spaces are safe.
pub fn windows_default_apps_plan(exe_path: &str, app_name: &str) -> WinDefaultAppsPlan {
    let caps = format!(r"{CAPS_ROOT}\Capabilities");
    let progid_root = format!(r"Software\Classes\{FOLDER_PROGID}");
    let reg_apps = r"Software\RegisteredApplications".to_string();
    let description = format!("Browse and open folders with {app_name}, a cross-platform file explorer.");

    let install = vec![
        // Publish the app so it appears in the Default-apps list. The value NAME is the display app name;
        // the value DATA is the HKCU-relative path of the Capabilities key.
        RegEntry { key: reg_apps.clone(), value_name: app_name.to_string(), value: caps.clone() },
        // Capabilities: what Windows shows + the associations we advertise.
        RegEntry { key: caps.clone(), value_name: "ApplicationName".into(), value: app_name.to_string() },
        RegEntry { key: caps.clone(), value_name: "ApplicationDescription".into(), value: description },
        // Advertise that our folder ProgID handles the folder ("Directory") class. Capability only — it
        // does NOT set the default (that stays the user's choice in Settings → Default apps).
        RegEntry {
            key: format!(r"{caps}\FileAssociations"),
            value_name: "Directory".into(),
            value: FOLDER_PROGID.to_string(),
        },
        // The folder ProgID: label, icon, and the command that launches CPE on the chosen folder (%1).
        RegEntry { key: progid_root.clone(), value_name: String::new(), value: format!("{app_name} Folder") },
        RegEntry { key: format!(r"{progid_root}\DefaultIcon"), value_name: String::new(), value: exe_path.to_string() },
        RegEntry {
            key: format!(r"{progid_root}\shell\open\command"),
            value_name: String::new(),
            value: format!(r#""{exe_path}" "%1""#),
        },
    ];
    WinDefaultAppsPlan {
        install,
        // Deleting these two roots removes the Capabilities subtree (incl. FileAssociations) and the whole
        // ProgID subtree (incl. DefaultIcon + shell\open\command).
        remove_keys: vec![CAPS_ROOT.to_string(), progid_root],
        // …and drop the single RegisteredApplications pointer value (a key delete there would nuke the
        // entries of every other app).
        remove_values: vec![RegValueRef { key: reg_apps, value_name: app_name.to_string() }],
    }
}

// ── Apply / remove the Windows plan against the real registry (CPE-1020, epic CPE-712) ──────────────
// Thin glue over `windows_shell_plan`: write every entry under HKCU on install, delete every root key on
// uninstall. All decision logic lives in the plan; this only executes it. Windows-only (behind `winreg`);
// other OSes get stubs so the contract compiles cross-platform (the 3-OS CI builds both branches).

#[cfg(windows)]
fn apply_entries(hkcu: &winreg::RegKey, entries: &[RegEntry]) -> Result<(), String> {
    for e in entries {
        let (key, _) = hkcu.create_subkey(&e.key).map_err(|err| format!("create {}: {err}", e.key))?;
        key.set_value(&e.value_name, &e.value)
            .map_err(|err| format!(r"set {}\{}: {err}", e.key, e.value_name))?;
    }
    Ok(())
}

#[cfg(windows)]
fn remove_keys(hkcu: &winreg::RegKey, keys: &[String]) -> Result<(), String> {
    for key in keys {
        match hkcu.delete_subkey_all(key) {
            Ok(()) => {}
            // Uninstall is idempotent: an already-absent key is success, not an error.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("delete {key}: {e}")),
        }
    }
    Ok(())
}

/// Install the Windows "Open in CPE" shell integration: write the plan for `exe_path` / `app_name` under
/// `HKCU\Software\Classes` (no elevation). Idempotent — re-installing overwrites the same values.
#[cfg(windows)]
pub fn install_shell_integration(exe_path: &str, app_name: &str) -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    apply_entries(&hkcu, &windows_shell_plan(exe_path, app_name).install)
}

/// Remove the Windows "Open in CPE" shell integration — delete every registered verb key (recursively).
/// Tolerates already-absent keys, so it's safe to call when nothing is installed.
#[cfg(windows)]
pub fn uninstall_shell_integration() -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    // The remove set is independent of exe/app name, so placeholders are fine.
    remove_keys(&hkcu, &windows_shell_plan("", "").remove)
}

/// Whether the integration is currently installed (the on-folder verb key exists).
#[cfg(windows)]
pub fn shell_integration_installed() -> bool {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    windows_shell_plan("", "").remove.first().is_some_and(|k| hkcu.open_subkey(k).is_ok())
}

// ── Apply / remove the Default-apps registration against the real registry (CPE-1277) ───────────────
// Same thin-glue pattern as the shell integration: install writes the plan's entries, uninstall deletes
// its key subtrees AND drops the RegisteredApplications pointer value (a key delete there would clobber
// every other registered app). Idempotent both ways; absent keys/values are treated as success.

/// Delete individual registry values (idempotent). Opens each parent key for write and drops the named
/// value; an absent parent key or value is success (already gone), so uninstall is safe to re-run.
#[cfg(windows)]
fn remove_values(hkcu: &winreg::RegKey, values: &[RegValueRef]) -> Result<(), String> {
    for v in values {
        match hkcu.open_subkey_with_flags(&v.key, winreg::enums::KEY_SET_VALUE) {
            Ok(key) => match key.delete_value(&v.value_name) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!(r"delete value {}\{}: {e}", v.key, v.value_name)),
            },
            // Parent key absent ⇒ the value is already gone.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("open {}: {e}", v.key)),
        }
    }
    Ok(())
}

/// Register CPE as a Windows Default-apps *candidate* for `exe_path` / `app_name` — writes the plan under
/// HKCU (no elevation). This never sets CPE as the default; the user confirms that in Settings → Default
/// apps. Idempotent — re-registering overwrites the same values.
#[cfg(windows)]
pub fn install_default_apps_registration(exe_path: &str, app_name: &str) -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    apply_entries(&hkcu, &windows_default_apps_plan(exe_path, app_name).install)
}

/// Remove the Default-apps registration — delete the Capabilities + ProgID subtrees and drop the
/// RegisteredApplications pointer. Tolerates already-absent keys/values, so it's safe when nothing is
/// registered. `app_name` names the RegisteredApplications value to drop.
#[cfg(windows)]
pub fn uninstall_default_apps_registration(app_name: &str) -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let plan = windows_default_apps_plan("", app_name);
    remove_keys(&hkcu, &plan.remove_keys)?;
    remove_values(&hkcu, &plan.remove_values)
}

/// Whether CPE is currently registered as a Default-apps candidate (the Capabilities key exists). Best
/// effort; name-independent so the Settings control can reflect true state without arguments.
#[cfg(windows)]
pub fn default_apps_registered() -> bool {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    hkcu.open_subkey(format!(r"{CAPS_ROOT}\Capabilities")).is_ok()
}

#[cfg(not(windows))]
pub fn install_default_apps_registration(_exe_path: &str, _app_name: &str) -> Result<(), String> {
    Err("default-apps registration is only implemented on Windows so far".into())
}

#[cfg(not(windows))]
pub fn uninstall_default_apps_registration(_app_name: &str) -> Result<(), String> {
    Err("default-apps registration is only implemented on Windows so far".into())
}

#[cfg(not(windows))]
pub fn default_apps_registered() -> bool {
    false
}

#[cfg(not(windows))]
pub fn install_shell_integration(_exe_path: &str, _app_name: &str) -> Result<(), String> {
    Err("shell-menu integration is only implemented on Windows so far".into())
}

#[cfg(not(windows))]
pub fn uninstall_shell_integration() -> Result<(), String> {
    Err("shell-menu integration is only implemented on Windows so far".into())
}

#[cfg(not(windows))]
pub fn shell_integration_installed() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verb(id: &str, applies: AppliesTo) -> MenuVerb {
        MenuVerb { id: id.into(), label: id.into(), command: format!("cpe {id} \"{{path}}\""), applies }
    }
    fn file(path: &str) -> SelItem {
        SelItem { path: path.into(), is_dir: false }
    }
    fn dir(path: &str) -> SelItem {
        SelItem { path: path.into(), is_dir: true }
    }

    #[test]
    fn any_always_applies_to_a_non_empty_selection() {
        let v = [verb("props", AppliesTo::Any)];
        assert_eq!(verbs_for(&v, &[file("a.txt"), dir("d")]).len(), 1);
        assert!(verbs_for(&v, &[]).is_empty()); // empty selection ⇒ nothing
    }

    #[test]
    fn files_and_folders_require_a_uniform_selection() {
        let vs = [verb("openfile", AppliesTo::Files), verb("openfolder", AppliesTo::Folders)];
        assert_eq!(verbs_for(&vs, &[file("a"), file("b")]).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["openfile"]);
        assert_eq!(verbs_for(&vs, &[dir("d1"), dir("d2")]).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["openfolder"]);
        assert!(verbs_for(&vs, &[file("a"), dir("d")]).is_empty()); // mixed ⇒ neither
    }

    #[test]
    fn extensions_match_every_selected_file_case_insensitively() {
        let v = [verb("edit-img", AppliesTo::Extensions(vec!["png".into(), ".JPG".into()]))];
        assert_eq!(verbs_for(&v, &[file("a.png"), file("b.JPG")]).len(), 1);
        assert!(verbs_for(&v, &[file("a.png"), file("c.gif")]).is_empty()); // one non-matching ⇒ hidden
        assert!(verbs_for(&v, &[dir("d.png")]).is_empty()); // a folder never matches an ext verb
    }

    #[test]
    fn registration_order_is_preserved() {
        let vs = [verb("z", AppliesTo::Any), verb("a", AppliesTo::Any)];
        assert_eq!(verbs_for(&vs, &[file("x")]).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["z", "a"]);
    }

    // ── Windows registration plan (CPE-1019) ──

    fn plan() -> WinShellPlan {
        windows_shell_plan(r"C:\Program Files\Cross-Platform Explorer\cpe.exe", "Cross-Platform Explorer")
    }

    fn cmd_for(p: &WinShellPlan, class: &str) -> String {
        let key = format!(r"Software\Classes\{class}\shell\CPE\command");
        p.install.iter().find(|e| e.key == key && e.value_name.is_empty()).map(|e| e.value.clone()).unwrap_or_default()
    }

    #[test]
    fn registers_the_three_explorer_surfaces_with_correct_command_placeholders() {
        let p = plan();
        // Folder and drive act on the clicked path (%1); background acts on the current dir (%V).
        assert_eq!(cmd_for(&p, "Directory"), r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%1""#);
        assert_eq!(cmd_for(&p, "Drive"), r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%1""#);
        assert_eq!(cmd_for(&p, r"Directory\Background"), r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%V""#);
    }

    #[test]
    fn label_and_icon_come_from_args() {
        let p = plan();
        let root = r"Software\Classes\Directory\shell\CPE";
        let default = p.install.iter().find(|e| e.key == root && e.value_name.is_empty()).unwrap();
        assert_eq!(default.value, "Open in Cross-Platform Explorer");
        let icon = p.install.iter().find(|e| e.key == root && e.value_name == "Icon").unwrap();
        assert_eq!(icon.value, r"C:\Program Files\Cross-Platform Explorer\cpe.exe");
    }

    #[test]
    fn every_installed_root_key_is_removed_on_uninstall() {
        // Reversibility invariant: no residue. Every `…\shell\CPE` root an install entry touches must be in
        // the remove set (strip a trailing `\command` to get each entry's verb root).
        let p = plan();
        for e in &p.install {
            let root = e.key.strip_suffix(r"\command").unwrap_or(&e.key);
            assert!(p.remove.contains(&root.to_string()), "install key {} has no matching remove entry", e.key);
        }
        assert_eq!(p.remove.len(), 3); // exactly the three surface roots, nothing stray
    }

    #[test]
    fn exe_path_is_quoted_so_spaces_are_safe() {
        // The command string must keep the exe as a single quoted token even with spaces in the path.
        assert!(cmd_for(&plan(), "Directory").starts_with(r#""C:\Program Files\"#));
    }

    // ── Linux plan (CPE-1021) ──

    #[test]
    fn linux_plan_writes_a_folder_desktop_entry_under_user_applications() {
        let p = linux_shell_plan("/opt/cpe/cpe", "Cross-Platform Explorer", "/home/sam");
        assert_eq!(p.desktop_path, "/home/sam/.local/share/applications/cross-platform-explorer.desktop");
        assert_eq!(p.mime_type, "inode/directory");
        // A valid launcher for folders, with the "Open in …" action.
        assert!(p.desktop_contents.contains("[Desktop Entry]"));
        assert!(p.desktop_contents.contains("Exec=/opt/cpe/cpe %F"));
        assert!(p.desktop_contents.contains("MimeType=inode/directory;"));
        assert!(p.desktop_contents.contains("[Desktop Action OpenInCPE]"));
        assert!(p.desktop_contents.contains("Name=Open in Cross-Platform Explorer"));
    }

    #[test]
    fn linux_plan_is_reversible() {
        let p = linux_shell_plan("/opt/cpe/cpe", "CPE", "/home/sam");
        // Everything installed (the one .desktop file) is in the remove set → no residue.
        assert!(p.remove_paths.contains(&p.desktop_path));
        assert_eq!(p.remove_paths.len(), 1);
    }

    // ── macOS plan (CPE-1022) ──

    #[test]
    fn macos_plan_emits_service_and_handler_fragments() {
        let p = macos_shell_plan("Cross-Platform Explorer", "com.cpe.app");
        // The Services entry carries the menu title + folder/file send-types.
        assert!(p.ns_service.contains("<string>Open in Cross-Platform Explorer</string>"));
        assert!(p.ns_service.contains("public.folder"));
        assert!(p.ns_service.contains("NSMessage"));
        // The LSHandlers association points folder-viewing at our bundle id.
        assert!(p.ls_handler.contains("<string>com.cpe.app</string>"));
        assert!(p.ls_handler.contains("public.folder"));
    }

    #[test]
    fn macos_plan_names_what_to_remove() {
        let p = macos_shell_plan("CPE", "com.cpe.app");
        // Uninstall must name the service menu title + the handler content type it added.
        assert!(p.remove_keys.contains(&"Open in CPE".to_string()));
        assert!(p.remove_keys.contains(&"public.folder".to_string()));
    }

    // ── apply / remove glue (CPE-1020) ── Exercises real winreg mechanics under an isolated scratch key
    // (never the live `…\shell\CPE` entries), so it neither pollutes the real shell menu nor needs elevation.
    #[cfg(windows)]
    #[test]
    fn apply_then_remove_roundtrips_and_is_idempotent() {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let base = format!(r"Software\CPE-test-{}", std::process::id());
        let root = format!(r"{base}\shell\CPE");
        let entries = vec![
            RegEntry { key: root.clone(), value_name: String::new(), value: "Open in Test".into() },
            RegEntry { key: root.clone(), value_name: "Icon".into(), value: r"x.exe".into() },
            RegEntry { key: format!(r"{root}\command"), value_name: String::new(), value: r#""x.exe" "%1""#.into() },
        ];

        // Install writes the values...
        apply_entries(&hkcu, &entries).expect("apply");
        let k = hkcu.open_subkey(&root).expect("root key exists after apply");
        let label: String = k.get_value("").expect("default value");
        let cmd: String = hkcu.open_subkey(format!(r"{root}\command")).unwrap().get_value("").unwrap();
        assert_eq!(label, "Open in Test");
        assert_eq!(cmd, r#""x.exe" "%1""#);

        // ...uninstall removes the whole subtree, and a second remove is a no-op (idempotent).
        remove_keys(&hkcu, std::slice::from_ref(&base)).expect("remove");
        remove_keys(&hkcu, std::slice::from_ref(&base)).expect("remove is idempotent when absent");
        assert!(hkcu.open_subkey(&base).is_err(), "scratch key should be gone after remove");
    }

    // ── Windows Default-apps registration plan (CPE-1277) ──

    fn dplan() -> WinDefaultAppsPlan {
        windows_default_apps_plan(r"C:\Program Files\Cross-Platform Explorer\cpe.exe", "Cross-Platform Explorer")
    }

    fn val<'a>(p: &'a WinDefaultAppsPlan, key: &str, name: &str) -> Option<&'a str> {
        p.install.iter().find(|e| e.key == key && e.value_name == name).map(|e| e.value.as_str())
    }

    #[test]
    fn default_apps_plan_publishes_the_app_under_registered_applications() {
        let p = dplan();
        // RegisteredApplications: value name = display name, data = the Capabilities key path.
        assert_eq!(
            val(&p, r"Software\RegisteredApplications", "Cross-Platform Explorer"),
            Some(r"Software\CrossPlatformExplorer\Capabilities")
        );
    }

    #[test]
    fn default_apps_plan_writes_capabilities_and_advertises_the_folder_progid() {
        let p = dplan();
        let caps = r"Software\CrossPlatformExplorer\Capabilities";
        assert_eq!(val(&p, caps, "ApplicationName"), Some("Cross-Platform Explorer"));
        assert!(val(&p, caps, "ApplicationDescription").unwrap().contains("Cross-Platform Explorer"));
        // FileAssociations advertises the folder ProgID for the "Directory" class (capability, not default).
        assert_eq!(
            val(&p, r"Software\CrossPlatformExplorer\Capabilities\FileAssociations", "Directory"),
            Some("CrossPlatformExplorer.Folder")
        );
    }

    #[test]
    fn default_apps_plan_registers_a_quoted_folder_open_command() {
        let p = dplan();
        let progid = r"Software\Classes\CrossPlatformExplorer.Folder";
        assert_eq!(val(&p, progid, ""), Some("Cross-Platform Explorer Folder"));
        assert_eq!(val(&p, &format!(r"{progid}\DefaultIcon"), ""), Some(r"C:\Program Files\Cross-Platform Explorer\cpe.exe"));
        // The launch command keeps the exe a single quoted token (spaces safe) and passes the folder (%1).
        let cmd = val(&p, &format!(r"{progid}\shell\open\command"), "").unwrap();
        assert_eq!(cmd, r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%1""#);
        assert!(cmd.starts_with(r#""C:\Program Files\"#));
    }

    #[test]
    fn default_apps_plan_never_force_sets_a_default() {
        // Honesty invariant: the plan only *registers* a candidate. It must not touch any HKCU location
        // that actually assigns a default handler — those live under `…\UserChoice`.
        let p = dplan();
        for e in &p.install {
            assert!(!e.key.contains("UserChoice"), "plan must not write a UserChoice (would force a default): {}", e.key);
        }
    }

    #[test]
    fn default_apps_plan_is_completely_reversible() {
        // Every installed entry must be reversed: either by a key-subtree delete (its key equals or nests
        // under a remove_keys root) or by an explicit value delete (the RegisteredApplications pointer).
        let p = dplan();
        for e in &p.install {
            let covered_by_key = p.remove_keys.iter().any(|k| e.key == *k || e.key.starts_with(&format!(r"{k}\")));
            let covered_by_value = p.remove_values.iter().any(|v| v.key == e.key && v.value_name == e.value_name);
            assert!(covered_by_key || covered_by_value, "install entry {}\\{} is not reversed on uninstall", e.key, e.value_name);
        }
        // Nothing stray: exactly the two subtree roots + the single pointer value.
        assert_eq!(p.remove_keys.len(), 2);
        assert_eq!(p.remove_values.len(), 1);
        assert_eq!(p.remove_values[0].key, r"Software\RegisteredApplications");
        assert_eq!(p.remove_values[0].value_name, "Cross-Platform Explorer");
    }

    // Exercises the real winreg apply → remove_keys + remove_values roundtrip under an ISOLATED scratch key,
    // never the live RegisteredApplications/Classes entries, so it pollutes nothing and needs no elevation.
    #[cfg(windows)]
    #[test]
    fn default_apps_apply_then_remove_values_roundtrips_and_is_idempotent() {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let base = format!(r"Software\CPE-dtest-{}", std::process::id());
        let reg_apps = format!(r"{base}\RegisteredApplications");
        let caps = format!(r"{base}\Caps");
        let entries = vec![
            RegEntry { key: reg_apps.clone(), value_name: "AppA".into(), value: caps.clone() },
            RegEntry { key: reg_apps.clone(), value_name: "AppB".into(), value: "keep-me".into() },
            RegEntry { key: caps.clone(), value_name: "ApplicationName".into(), value: "A".into() },
        ];
        apply_entries(&hkcu, &entries).expect("apply");

        // remove_values drops only the named value; the sibling ("AppB") survives.
        let vals = vec![RegValueRef { key: reg_apps.clone(), value_name: "AppA".into() }];
        remove_values(&hkcu, &vals).expect("remove_values");
        remove_values(&hkcu, &vals).expect("remove_values is idempotent when absent");
        let ra = hkcu.open_subkey(&reg_apps).expect("RegisteredApplications key still exists");
        assert!(ra.get_value::<String, _>("AppA").is_err(), "AppA value should be gone");
        assert_eq!(ra.get_value::<String, _>("AppB").unwrap(), "keep-me", "sibling value must survive");

        // remove_keys clears the subtree; remove_values on an absent parent is a no-op (not an error).
        remove_keys(&hkcu, std::slice::from_ref(&base)).expect("remove scratch subtree");
        remove_values(&hkcu, &vals).expect("remove_values tolerates an absent parent key");
        assert!(hkcu.open_subkey(&base).is_err(), "scratch key should be gone");
    }
}
