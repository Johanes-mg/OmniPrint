use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const WORKER_FLAG: &str = "--admin-worker";

pub fn run_worker(args: &[String]) -> i32 {
    if args.len() < 2 {
        return 2;
    }
    let script_path = Path::new(&args[0]);
    let done_path = Path::new(&args[1]);

    let est_ps1 = script_path
        .extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("ps1"))
        .unwrap_or(false);

    let resultat = if est_ps1 {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ])
            .arg(script_path)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    } else {
        Command::new("cmd")
            .arg("/c")
            .arg(script_path)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };

    let (code, message) = match resultat {
        Ok(o) if o.status.success() => (0, "OK".to_string()),
        Ok(o) => (1, format!("ERR: code {}", o.status.code().unwrap_or(-1))),
        Err(e) => (1, format!("ERR: {}", e)),
    };

    let _ = std::fs::write(done_path, &message);
    code
}

pub fn executer_en_admin(script: &Path) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Exécutable introuvable : {}", e))?;
    let id = uuid::Uuid::new_v4();
    let done = std::env::temp_dir().join(format!("omniprint_done_{}.txt", id));

    let exe_s = exe.display().to_string().replace('\'', "''");
    let script_s = script.display().to_string().replace('\'', "''");
    let done_s = done.display().to_string().replace('\'', "''");

    let cmd = format!(
        "Start-Process -FilePath '{}' -ArgumentList '{}','{}','{}' -Verb RunAs -Wait",
        exe_s, WORKER_FLAG, script_s, done_s
    );

    let sortie = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &cmd,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let done_exist = done.exists();
    let contenu = if done_exist {
        std::fs::read_to_string(&done).unwrap_or_default()
    } else {
        String::new()
    };
    let _ = std::fs::remove_file(&done);

    match sortie {
        Ok(_) if done_exist && contenu.starts_with("OK") => Ok(()),
        Ok(_) => Err(if contenu.is_empty() {
            "Élévation refusée.".into()
        } else {
            contenu
        }),
        Err(e) => Err(format!("PowerShell indisponible : {}", e)),
    }
}
