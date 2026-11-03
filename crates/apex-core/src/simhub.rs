//! Installation du plugin SimHub d'Apex (repères de freinage pour les LEDs du volant).
//!
//! Le plugin est livré en code source et compilé sur le PC du pilote avec le compilateur C# fourni par
//! Windows (.NET Framework 4, `csc.exe`), contre les bibliothèques de SimHub déjà installées : aucune
//! bibliothèque de SimHub n'est redistribuée avec Apex, et le plugin suit la version de SimHub du pilote.

use std::path::{Path, PathBuf};

pub const SOURCE: &str = include_str!("../../../simhub/ApexCompanion.cs");
pub const DLL_NAME: &str = "ApexCompanion.SimHub.dll";
/// Bibliothèques de SimHub nécessaires à la compilation.
pub const REFS: [&str; 2] = ["SimHub.Plugins.dll", "GameReaderCommon.dll"];

/// Dossier d'installation de SimHub : celui choisi par le pilote, sinon les emplacements habituels.
pub fn find_simhub(chosen: Option<&Path>) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = chosen.map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(pf) = std::env::var_os(var) {
            cands.push(PathBuf::from(pf).join("SimHub"));
        }
    }
    cands.push(PathBuf::from(r"C:\Program Files (x86)\SimHub"));
    cands.into_iter().find(|d| REFS.iter().all(|r| d.join(r).exists()))
}

/// Compilateur C# de Windows (.NET Framework 4.x).
pub fn find_csc() -> Option<PathBuf> {
    let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    ["Framework64", "Framework"].iter().map(|f| win.join("Microsoft.NET").join(f).join("v4.0.30319").join("csc.exe")).find(|p| p.exists())
}

/// Arguments de compilation (testables hors Windows).
pub fn csc_args(simhub: &Path, src: &Path, out: &Path) -> Vec<String> {
    let mut a = vec!["/nologo".to_string(), "/target:library".into(), "/optimize+".into(), format!("/out:{}", out.display())];
    for r in REFS {
        a.push(format!("/reference:{}", simhub.join(r).display()));
    }
    a.push("/reference:System.dll".into());
    a.push("/reference:System.Core.dll".into());
    a.push(src.display().to_string());
    a
}

/// Compile le plugin dans `work` : renvoie le chemin de la DLL, ou le message du compilateur.
pub fn build(simhub: &Path, work: &Path) -> Result<PathBuf, String> {
    let csc = find_csc().ok_or("compilateur C# de Windows introuvable (.NET Framework 4)")?;
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let src = work.join("ApexCompanion.cs");
    std::fs::write(&src, SOURCE).map_err(|e| e.to_string())?;
    let out = work.join(DLL_NAME);
    let mut cmd = std::process::Command::new(csc);
    cmd.args(csc_args(simhub, &src, &out));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let o = cmd.output().map_err(|e| format!("lancement du compilateur : {e}"))?;
    if !o.status.success() || !out.exists() {
        let log = String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr);
        let errors: Vec<&str> = log.lines().filter(|l| l.contains("error")).take(6).collect();
        return Err(format!("compilation du plugin impossible : {}", if errors.is_empty() { log.trim().to_string() } else { errors.join(" | ") }));
    }
    Ok(out)
}

/// Copie la DLL dans le dossier de SimHub. Dossier protégé (Program Files) : copie avec élévation
/// (Windows demande l'autorisation une fois).
pub fn install(dll: &Path, simhub: &Path) -> Result<PathBuf, String> {
    let dest = simhub.join(DLL_NAME);
    match std::fs::copy(dll, &dest) {
        Ok(_) => Ok(dest),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => elevated_copy(dll, &dest).map(|_| dest),
        Err(e) => Err(format!("copie dans le dossier de SimHub : {e} (SimHub est peut-être ouvert : ferme-le puis réessaie)")),
    }
}

#[cfg(windows)]
fn elevated_copy(src: &Path, dest: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::Foundation::CloseHandle;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb = wide("runas");
    let file = wide("cmd.exe");
    let params = wide(&format!("/c copy /Y \"{}\" \"{}\"", src.display(), dest.display()));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = 0;
        if ShellExecuteExW(&mut info) == 0 {
            return Err("copie refusée (autorisation administrateur non accordée)".into());
        }
        if !info.hProcess.is_null() {
            WaitForSingleObject(info.hProcess, 30_000);
            CloseHandle(info.hProcess);
        }
    }
    if dest.exists() { Ok(()) } else { Err("copie dans le dossier de SimHub non effectuée".into()) }
}

#[cfg(not(windows))]
fn elevated_copy(_src: &Path, _dest: &Path) -> Result<(), String> {
    Err("copie protégée : Windows uniquement".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_source_is_plain_csharp5_and_matches_the_led_port() {
        assert!(SOURCE.contains(&format!("Port = {}", crate::led::DEFAULT_PORT)), "port du plugin ≠ port d'Apex");
        // Le compilateur de Windows (C# 5) ne connaît pas ces syntaxes.
        for bad in ["$\"", "?.", "=> new", "nameof(", "out var"] {
            assert!(!SOURCE.contains(bad), "syntaxe trop récente pour csc.exe : {bad}");
        }
        assert!(SOURCE.contains("\"APEX1\""));
    }

    #[test]
    fn compiler_arguments_reference_simhub() {
        let a = csc_args(Path::new("/sh"), Path::new("/w/a.cs"), Path::new("/w/o.dll"));
        assert!(a.iter().any(|x| x.ends_with("SimHub.Plugins.dll")));
        assert!(a.iter().any(|x| x.ends_with("GameReaderCommon.dll")));
        assert_eq!(a.last().unwrap(), "/w/a.cs");
    }
}
