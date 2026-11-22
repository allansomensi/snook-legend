//! Country flags from the bundled atlas (flag-icons, MIT licence).

use std::cell::OnceCell;

use macroquad::prelude::*;
use snook_legend_proto::{COUNTRIES, FLAG_COLUMNS, FLAG_H, FLAG_W, country_index};

thread_local! {
    static ATLAS: OnceCell<Option<Texture2D>> = const { OnceCell::new() };
}

fn atlas() -> Option<Texture2D> {
    ATLAS.with(|a| {
        a.get_or_init(|| {
            let tex = Texture2D::from_file_with_format(include_bytes!("../assets/flags.png"), None);
            tex.set_filter(FilterMode::Linear);
            Some(tex)
        })
        .clone()
    })
}

/// Draws the flag of `code` with its top-left corner at (`x`, `y`) and the
/// given height (4:3). Nothing is drawn for an unknown or empty code.
pub fn draw(code: &str, x: f32, y: f32, h: f32) -> bool {
    let Some(i) = country_index(code) else { return false };
    let Some(tex) = atlas() else { return false };
    let (col, row) = (i as u32 % FLAG_COLUMNS, i as u32 / FLAG_COLUMNS);
    let src = Rect::new((col * FLAG_W) as f32, (row * FLAG_H) as f32, FLAG_W as f32, FLAG_H as f32);
    let w = h * 4.0 / 3.0;
    let a = crate::paint::alpha();
    crate::paint::draw_rectangle(
        x - 0.5,
        y - 0.5,
        w + 1.0,
        h + 1.0,
        Color::new(0.0, 0.0, 0.0, 0.5),
    );
    draw_texture_ex(
        &tex,
        x,
        y,
        Color::new(1.0, 1.0, 1.0, a),
        DrawTextureParams { dest_size: Some(vec2(w, h)), source: Some(src), ..Default::default() },
    );
    true
}

/// Width of a flag drawn `h` high.
pub fn width(h: f32) -> f32 {
    h * 4.0 / 3.0
}

/// The country's name in the interface language.
pub fn name(code: &str) -> &'static str {
    use crate::i18n::Lang;
    let Some(i) = country_index(code) else { return "" };
    let lang = crate::i18n::lang();
    match lang {
        Lang::English => COUNTRIES[i].1,
        Lang::Portuguese => COUNTRIES[i].2,
        Lang::Spanish if spanish(COUNTRIES[i].0).is_some() => {
            spanish(COUNTRIES[i].0).unwrap_or(COUNTRIES[i].1)
        }
        _ => crate::country_names::NAMES
            .iter()
            .find(|(l, _)| *l == lang.code())
            .and_then(|(_, names)| names.get(i))
            .filter(|n| !n.is_empty())
            .copied()
            .unwrap_or(COUNTRIES[i].1),
    }
}

/// Spanish names where they differ from English; English otherwise.
pub fn spanish(code: &str) -> Option<&'static str> {
    Some(match code {
        "AR" => "Argentina",
        "BO" => "Bolivia",
        "BR" => "Brasil",
        "CL" => "Chile",
        "CO" => "Colombia",
        "CR" => "Costa Rica",
        "CU" => "Cuba",
        "DO" => "República Dominicana",
        "EC" => "Ecuador",
        "SV" => "El Salvador",
        "GT" => "Guatemala",
        "HN" => "Honduras",
        "MX" => "México",
        "NI" => "Nicaragua",
        "PA" => "Panamá",
        "PY" => "Paraguay",
        "PE" => "Perú",
        "PR" => "Puerto Rico",
        "UY" => "Uruguay",
        "VE" => "Venezuela",
        "US" => "Estados Unidos",
        "CA" => "Canadá",
        "ES" => "España",
        "PT" => "Portugal",
        "FR" => "Francia",
        "DE" => "Alemania",
        "IT" => "Italia",
        "GB" => "Reino Unido",
        "IE" => "Irlanda",
        "NL" => "Países Bajos",
        "BE" => "Bélgica",
        "CH" => "Suiza",
        "AT" => "Austria",
        "SE" => "Suecia",
        "NO" => "Noruega",
        "DK" => "Dinamarca",
        "FI" => "Finlandia",
        "PL" => "Polonia",
        "RU" => "Rusia",
        "UA" => "Ucrania",
        "TR" => "Turquía",
        "GR" => "Grecia",
        "JP" => "Japón",
        "CN" => "China",
        "KR" => "Corea del Sur",
        "IN" => "India",
        "PH" => "Filipinas",
        "AU" => "Australia",
        "NZ" => "Nueva Zelanda",
        "ZA" => "Sudáfrica",
        "EG" => "Egipto",
        "MA" => "Marruecos",
        "AO" => "Angola",
        "MZ" => "Mozambique",
        "CV" => "Cabo Verde",
        "GQ" => "Guinea Ecuatorial",
        // The rest of the world (from the ISO 3166 names in Spanish, in
        // their everyday form).
        "AE" => "Emiratos Árabes Unidos",
        "AF" => "Afganistán",
        "AG" => "Antigua y Barbuda",
        "AI" => "Anguila",
        "AQ" => "Antártida",
        "AS" => "Samoa Estadounidense",
        "AX" => "Islas Åland",
        "AZ" => "Azerbaiyán",
        "BA" => "Bosnia y Herzegovina",
        "BD" => "Bangladés",
        "BH" => "Baréin",
        "BJ" => "Benín",
        "BL" => "San Bartolomé",
        "BM" => "Bermudas",
        "BN" => "Brunéi",
        "BQ" => "Caribe Neerlandés",
        "BT" => "Bután",
        "BV" => "Isla Bouvet",
        "BW" => "Botsuana",
        "BY" => "Bielorrusia",
        "BZ" => "Belice",
        "CC" => "Islas Cocos (Keeling)",
        "CD" => "República Democrática del Congo",
        "CF" => "República Centroafricana",
        "CG" => "República del Congo",
        "CI" => "Costa de Marfil",
        "CK" => "Islas Cook",
        "CM" => "Camerún",
        "CW" => "Curazao",
        "CX" => "Isla de Navidad",
        "CY" => "Chipre",
        "CZ" => "Chequia",
        "DJ" => "Yibuti",
        "DZ" => "Argelia",
        "EH" => "Sahara Occidental",
        "ET" => "Etiopía",
        "FJ" => "Fiyi",
        "FK" => "Islas Malvinas",
        "FM" => "Micronesia",
        "FO" => "Islas Feroe",
        "GA" => "Gabón",
        "GD" => "Granada",
        "GF" => "Guayana Francesa",
        "GL" => "Groenlandia",
        "GP" => "Guadalupe",
        "GW" => "Guinea-Bisáu",
        "HM" => "Islas Heard y McDonald",
        "HR" => "Croacia",
        "HT" => "Haití",
        "HU" => "Hungría",
        "IM" => "Isla de Man",
        "IO" => "Territorio Británico del Océano Índico",
        "IQ" => "Irak",
        "IR" => "Irán",
        "IS" => "Islandia",
        "JO" => "Jordania",
        "KE" => "Kenia",
        "KG" => "Kirguistán",
        "KH" => "Camboya",
        "KM" => "Comoras",
        "KN" => "San Cristóbal y Nieves",
        "KP" => "Corea del Norte",
        "KY" => "Islas Caimán",
        "KZ" => "Kazajistán",
        "LB" => "Líbano",
        "LC" => "Santa Lucía",
        "LS" => "Lesoto",
        "LT" => "Lituania",
        "LU" => "Luxemburgo",
        "LV" => "Letonia",
        "LY" => "Libia",
        "MC" => "Mónaco",
        "MD" => "Moldavia",
        "MF" => "San Martín (Francia)",
        "MH" => "Islas Marshall",
        "MK" => "Macedonia del Norte",
        "ML" => "Malí",
        "MP" => "Islas Marianas del Norte",
        "MQ" => "Martinica",
        "MU" => "Mauricio",
        "MV" => "Maldivas",
        "MW" => "Malaui",
        "MY" => "Malasia",
        "NC" => "Nueva Caledonia",
        "NE" => "Níger",
        "NF" => "Isla Norfolk",
        "OM" => "Omán",
        "PF" => "Polinesia Francesa",
        "PG" => "Papúa Nueva Guinea",
        "PK" => "Pakistán",
        "PM" => "San Pedro y Miquelon",
        "PN" => "Islas Pitcairn",
        "PS" => "Palestina",
        "PW" => "Palaos",
        "QA" => "Catar",
        "RE" => "Reunión",
        "RO" => "Rumanía",
        "RW" => "Ruanda",
        "SA" => "Arabia Saudita",
        "SB" => "Islas Salomón",
        "SD" => "Sudán",
        "SG" => "Singapur",
        "SI" => "Eslovenia",
        "SJ" => "Svalbard y Jan Mayen",
        "SK" => "Eslovaquia",
        "SL" => "Sierra Leona",
        "SR" => "Surinam",
        "SS" => "Sudán del Sur",
        "ST" => "Santo Tomé y Príncipe",
        "SX" => "San Martín (Países Bajos)",
        "SY" => "Siria",
        "SZ" => "Esuatini",
        "TC" => "Islas Turcas y Caicos",
        "TF" => "Territorios Franceses del Sur",
        "TH" => "Tailandia",
        "TJ" => "Tayikistán",
        "TL" => "Timor Oriental",
        "TM" => "Turkmenistán",
        "TN" => "Túnez",
        "TT" => "Trinidad y Tobago",
        "TW" => "Taiwán",
        "UM" => "Islas menores alejadas de EE. UU.",
        "UZ" => "Uzbekistán",
        "VA" => "Ciudad del Vaticano",
        "VC" => "San Vicente y las Granadinas",
        "VG" => "Islas Vírgenes Británicas",
        "VI" => "Islas Vírgenes de EE. UU.",
        "WF" => "Wallis y Futuna",
        "ZW" => "Zimbabue",
        _ => return None,
    })
}

/// The country of the operating system's locale ("pt-BR" → "BR").
pub fn system_country() -> String {
    sys_locale::get_locale()
        .and_then(|l| {
            l.split(['-', '_'])
                .nth(1)
                .map(|c| c.to_uppercase())
                .filter(|c| c.len() == 2 && country_index(c).is_some())
        })
        .unwrap_or_default()
}
