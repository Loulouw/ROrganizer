//! Map a Dofus class string (extracted from the window title via the
//! configured regex) to one of the 19 PNG filenames embedded in
//! `resources/classes/`. Covers the FR / EN / ES names returned by the
//! official Ankama encyclopedia, since the title language follows the
//! Dofus client locale, not the OS or ROrganizer UI language.

pub fn class_filename(raw: &str) -> Option<&'static str> {
    match normalize(raw).as_str() {
        "feca" => Some("feca"),
        "osamodas" => Some("osamodas"),
        "enutrof" | "anutrof" => Some("enutrof"),
        "sram" => Some("sram"),
        "xelor" => Some("xelor"),
        "ecaflip" | "zurcarak" => Some("ecaflip"),
        "eniripsa" | "aniripsa" => Some("eniripsa"),
        "iop" | "yopuka" => Some("iop"),
        "cra" | "ocra" => Some("cra"),
        "sadida" => Some("sadida"),
        "sacrieur" | "sacrier" | "sacrogrito" => Some("sacrieur"),
        "pandawa" => Some("pandawa"),
        "roublard" | "rogue" | "tymadore" => Some("roublard"),
        "zobal" | "masqueraider" => Some("zobal"),
        "steamer" | "foggernaut" => Some("steamer"),
        "eliotrope" | "selotrope" => Some("eliotrope"),
        "huppermage" | "hipermagos" => Some("huppermage"),
        "ouginak" | "uginak" => Some("ouginak"),
        "forgelance" | "forjalanza" => Some("forgelance"),
        _ => None,
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .map(strip_accent)
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_ascii_alphabetic())
        .collect()
}

fn strip_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr_accented_names() {
        assert_eq!(class_filename("Crâ"), Some("cra"));
        assert_eq!(class_filename("Féca"), Some("feca"));
        assert_eq!(class_filename("Xélor"), Some("xelor"));
    }

    #[test]
    fn en_aliases() {
        assert_eq!(class_filename("Rogue"), Some("roublard"));
        assert_eq!(class_filename("Foggernaut"), Some("steamer"));
        assert_eq!(class_filename("Masqueraider"), Some("zobal"));
        assert_eq!(class_filename("Sacrier"), Some("sacrieur"));
    }

    #[test]
    fn es_aliases() {
        assert_eq!(class_filename("Yopuka"), Some("iop"));
        assert_eq!(class_filename("Sacrogrito"), Some("sacrieur"));
        assert_eq!(class_filename("Tymadore"), Some("roublard"));
        assert_eq!(class_filename("Forjalanza"), Some("forgelance"));
        assert_eq!(class_filename("Aniripsa"), Some("eniripsa"));
        assert_eq!(class_filename("Anutrof"), Some("enutrof"));
        assert_eq!(class_filename("Hipermagos"), Some("huppermage"));
        assert_eq!(class_filename("Selotrope"), Some("eliotrope"));
        assert_eq!(class_filename("Uginak"), Some("ouginak"));
        assert_eq!(class_filename("Zurcarak"), Some("ecaflip"));
        assert_eq!(class_filename("Ocra"), Some("cra"));
    }

    #[test]
    fn case_and_whitespace_insensitive() {
        assert_eq!(class_filename("FECA"), Some("feca"));
        assert_eq!(class_filename(" iop "), Some("iop"));
        assert_eq!(class_filename("Sram!"), Some("sram"));
    }

    #[test]
    fn unknown_class() {
        assert_eq!(class_filename("InconnuClass"), None);
        assert_eq!(class_filename(""), None);
    }
}
