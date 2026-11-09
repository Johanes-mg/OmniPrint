use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModePages {
    Toutes,
    Personnalise,
}

impl Default for ModePages {
    fn default() -> Self {
        ModePages::Toutes
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectionPages {
    pub mode: ModePages,
    pub pages_cochees: Vec<u32>,
    pub copies: u32,
}

impl Default for SelectionPages {
    fn default() -> Self {
        Self {
            mode: ModePages::Toutes,
            pages_cochees: Vec::new(),
            copies: 1,
        }
    }
}

impl SelectionPages {
    pub fn resoudre(&self, total: u32) -> Result<Vec<u32>, String> {
        match self.mode {
            ModePages::Toutes => Ok((1..=total).collect()),
            ModePages::Personnalise => {
                if self.pages_cochees.is_empty() {
                    return Err("Aucune page sélectionnée.".into());
                }
                let mut v: Vec<u32> = self
                    .pages_cochees
                    .iter()
                    .copied()
                    .filter(|p| *p >= 1 && *p <= total)
                    .collect();
                v.sort_unstable();
                v.dedup();
                if v.is_empty() {
                    return Err("Aucune page valide.".into());
                }
                Ok(v)
            }
        }
    }

    pub fn spec_imprimante(&self, total: u32) -> Result<String, String> {
        let pages = self.resoudre(total)?;
        if self.mode == ModePages::Toutes {
            return Ok(String::new());
        }
        Ok(formater_pages(&pages))
    }

    pub fn copies_effectives(&self) -> u32 {
        if self.copies == 0 { 1 } else { self.copies }
    }

    pub fn pages_effectives(&self, total: u32) -> u32 {
        match self.mode {
            ModePages::Toutes => total,
            ModePages::Personnalise => {
                self.pages_cochees
                    .iter()
                    .filter(|p| **p >= 1 && **p <= total)
                    .count() as u32
            }
        }
    }

    pub fn resume(&self, total: u32) -> String {
        match self.mode {
            ModePages::Toutes => {
                if total > 0 {
                    format!("Toutes ({} pages)", total)
                } else {
                    "Toutes les pages".into()
                }
            },
            ModePages::Personnalise => {
                if self.pages_cochees.is_empty() {
                    "Sélection vide".into()
                } else {
                    let mut v = self.pages_cochees.clone();
                    v.sort_unstable();
                    v.dedup();
                    format!("Sélection : {}", formater_pages(&v))
                }
            }
        }
    }
}

pub fn formater_pages(pages: &[u32]) -> String {
    if pages.is_empty() {
        return String::new();
    }
    let mut triees = pages.to_vec();
    triees.sort_unstable();
    triees.dedup();
    let mut morceaux: Vec<String> = Vec::new();
    let mut i = 0;
    while i < triees.len() {
        let debut = triees[i];
        let mut fin = debut;
        while i + 1 < triees.len() && triees[i + 1] == fin + 1 {
            fin = triees[i + 1];
            i += 1;
        }
        if debut == fin {
            morceaux.push(format!("{}", debut));
        } else {
            morceaux.push(format!("{}-{}", debut, fin));
        }
        i += 1;
    }
    morceaux.join(",")
}
