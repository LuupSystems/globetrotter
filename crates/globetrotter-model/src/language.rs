use serde::{Deserialize, Serialize};

// spellcheck:ignore-block
/// Language codes per ISO 639-1 Alpha-2
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
    strum::VariantNames,
    strum::IntoStaticStr,
    strum::EnumCount,
    strum::EnumIter,
)]
pub enum Language {
    #[serde(rename = "aa")]
    #[strum(to_string = "aa", serialize = "aa")]
    Aa,
    #[serde(rename = "ab")]
    #[strum(to_string = "ab", serialize = "ab")]
    Ab,
    #[serde(rename = "af")]
    #[strum(to_string = "af", serialize = "af")]
    Af,
    #[serde(rename = "ak")]
    #[strum(to_string = "ak", serialize = "ak")]
    Ak,
    #[serde(rename = "am")]
    #[strum(to_string = "am", serialize = "am")]
    Am,
    #[serde(rename = "an")]
    #[strum(to_string = "an", serialize = "an")]
    An,
    #[serde(rename = "ar")]
    #[strum(to_string = "ar", serialize = "ar")]
    Ar,
    #[serde(rename = "as")]
    #[strum(to_string = "as", serialize = "as")]
    As,
    #[serde(rename = "av")]
    #[strum(to_string = "av", serialize = "av")]
    Av,
    #[serde(rename = "ay")]
    #[strum(to_string = "ay", serialize = "ay")]
    Ay,
    #[serde(rename = "az")]
    #[strum(to_string = "az", serialize = "az")]
    Az,
    #[serde(rename = "ba")]
    #[strum(to_string = "ba", serialize = "ba")]
    Ba,
    #[serde(rename = "be")]
    #[strum(to_string = "be", serialize = "be")]
    Be,
    #[serde(rename = "bg")]
    #[strum(to_string = "bg", serialize = "bg")]
    Bg,
    #[serde(rename = "bh")]
    #[strum(to_string = "bh", serialize = "bh")]
    Bh,
    #[serde(rename = "bi")]
    #[strum(to_string = "bi", serialize = "bi")]
    Bi,
    #[serde(rename = "bm")]
    #[strum(to_string = "bm", serialize = "bm")]
    Bm,
    #[serde(rename = "bn")]
    #[strum(to_string = "bn", serialize = "bn")]
    Bn,
    #[serde(rename = "bo")]
    #[strum(to_string = "bo", serialize = "bo")]
    Bo,
    #[serde(rename = "br")]
    #[strum(to_string = "br", serialize = "br")]
    Br,
    #[serde(rename = "bs")]
    #[strum(to_string = "bs", serialize = "bs")]
    Bs,
    #[serde(rename = "ca")]
    #[strum(to_string = "ca", serialize = "ca")]
    Ca,
    #[serde(rename = "ce")]
    #[strum(to_string = "ce", serialize = "ce")]
    Ce,
    #[serde(rename = "ch")]
    #[strum(to_string = "ch", serialize = "ch")]
    Ch,
    #[serde(rename = "co")]
    #[strum(to_string = "co", serialize = "co")]
    Co,
    #[serde(rename = "cr")]
    #[strum(to_string = "cr", serialize = "cr")]
    Cr,
    #[serde(rename = "cs")]
    #[strum(to_string = "cs", serialize = "cs")]
    Cs,
    #[serde(rename = "cu")]
    #[strum(to_string = "cu", serialize = "cu")]
    Cu,
    #[serde(rename = "cv")]
    #[strum(to_string = "cv", serialize = "cv")]
    Cv,
    #[serde(rename = "cy")]
    #[strum(to_string = "cy", serialize = "cy")]
    Cy,
    #[serde(rename = "da")]
    #[strum(to_string = "da", serialize = "da")]
    Da,
    #[serde(rename = "de")]
    #[strum(to_string = "de", serialize = "de")]
    De,
    #[serde(rename = "dv")]
    #[strum(to_string = "dv", serialize = "dv")]
    Dv,
    #[serde(rename = "dz")]
    #[strum(to_string = "dz", serialize = "dz")]
    Dz,
    #[serde(rename = "ee")]
    #[strum(to_string = "ee", serialize = "ee")]
    Ee,
    #[serde(rename = "el")]
    #[strum(to_string = "el", serialize = "el")]
    El,
    #[serde(rename = "en")]
    #[strum(to_string = "en", serialize = "en")]
    En,
    #[serde(rename = "eo")]
    #[strum(to_string = "eo", serialize = "eo")]
    Eo,
    #[serde(rename = "es")]
    #[strum(to_string = "es", serialize = "es")]
    Es,
    #[serde(rename = "et")]
    #[strum(to_string = "et", serialize = "et")]
    Et,
    #[serde(rename = "eu")]
    #[strum(to_string = "eu", serialize = "eu")]
    Eu,
    #[serde(rename = "fa")]
    #[strum(to_string = "fa", serialize = "fa")]
    Fa,
    #[serde(rename = "ff")]
    #[strum(to_string = "ff", serialize = "ff")]
    Ff,
    #[serde(rename = "fi")]
    #[strum(to_string = "fi", serialize = "fi")]
    Fi,
    #[serde(rename = "fj")]
    #[strum(to_string = "fj", serialize = "fj")]
    Fj,
    #[serde(rename = "fo")]
    #[strum(to_string = "fo", serialize = "fo")]
    Fo,
    #[serde(rename = "fr")]
    #[strum(to_string = "fr", serialize = "fr")]
    Fr,
    #[serde(rename = "fy")]
    #[strum(to_string = "fy", serialize = "fy")]
    Fy,
    #[serde(rename = "ga")]
    #[strum(to_string = "ga", serialize = "ga")]
    Ga,
    #[serde(rename = "gd")]
    #[strum(to_string = "gd", serialize = "gd")]
    Gd,
    #[serde(rename = "gl")]
    #[strum(to_string = "gl", serialize = "gl")]
    Gl,
    #[serde(rename = "gn")]
    #[strum(to_string = "gn", serialize = "gn")]
    Gn,
    #[serde(rename = "gu")]
    #[strum(to_string = "gu", serialize = "gu")]
    Gu,
    #[serde(rename = "gv")]
    #[strum(to_string = "gv", serialize = "gv")]
    Gv,
    #[serde(rename = "ha")]
    #[strum(to_string = "ha", serialize = "ha")]
    Ha,
    #[serde(rename = "he")]
    #[strum(to_string = "he", serialize = "he")]
    He,
    #[serde(rename = "hi")]
    #[strum(to_string = "hi", serialize = "hi")]
    Hi,
    #[serde(rename = "ho")]
    #[strum(to_string = "ho", serialize = "ho")]
    Ho,
    #[serde(rename = "hr")]
    #[strum(to_string = "hr", serialize = "hr")]
    Hr,
    #[serde(rename = "ht")]
    #[strum(to_string = "ht", serialize = "ht")]
    Ht,
    #[serde(rename = "hu")]
    #[strum(to_string = "hu", serialize = "hu")]
    Hu,
    #[serde(rename = "hy")]
    #[strum(to_string = "hy", serialize = "hy")]
    Hy,
    #[serde(rename = "hz")]
    #[strum(to_string = "hz", serialize = "hz")]
    Hz,
    #[serde(rename = "ia")]
    #[strum(to_string = "ia", serialize = "ia")]
    Ia,
    #[serde(rename = "id")]
    #[strum(to_string = "id", serialize = "id")]
    Id,
    #[serde(rename = "ie")]
    #[strum(to_string = "ie", serialize = "ie")]
    Ie,
    #[serde(rename = "ig")]
    #[strum(to_string = "ig", serialize = "ig")]
    Ig,
    #[serde(rename = "ii")]
    #[strum(to_string = "ii", serialize = "ii")]
    Ii,
    #[serde(rename = "ik")]
    #[strum(to_string = "ik", serialize = "ik")]
    Ik,
    #[serde(rename = "io")]
    #[strum(to_string = "io", serialize = "io")]
    Io,
    #[serde(rename = "is")]
    #[strum(to_string = "is", serialize = "is")]
    Is,
    #[serde(rename = "it")]
    #[strum(to_string = "it", serialize = "it")]
    It,
    #[serde(rename = "iu")]
    #[strum(to_string = "iu", serialize = "iu")]
    Iu,
    #[serde(rename = "ja")]
    #[strum(to_string = "ja", serialize = "ja")]
    Ja,
    #[serde(rename = "jv")]
    #[strum(to_string = "jv", serialize = "jv")]
    Jv,
    #[serde(rename = "ka")]
    #[strum(to_string = "ka", serialize = "ka")]
    Ka,
    #[serde(rename = "kg")]
    #[strum(to_string = "kg", serialize = "kg")]
    Kg,
    #[serde(rename = "ki")]
    #[strum(to_string = "ki", serialize = "ki")]
    Ki,
    #[serde(rename = "kj")]
    #[strum(to_string = "kj", serialize = "kj")]
    Kj,
    #[serde(rename = "kk")]
    #[strum(to_string = "kk", serialize = "kk")]
    Kk,
    #[serde(rename = "kl")]
    #[strum(to_string = "kl", serialize = "kl")]
    Kl,
    #[serde(rename = "km")]
    #[strum(to_string = "km", serialize = "km")]
    Km,
    #[serde(rename = "kn")]
    #[strum(to_string = "kn", serialize = "kn")]
    Kn,
    #[serde(rename = "ko")]
    #[strum(to_string = "ko", serialize = "ko")]
    Ko,
    #[serde(rename = "kr")]
    #[strum(to_string = "kr", serialize = "kr")]
    Kr,
    #[serde(rename = "ks")]
    #[strum(to_string = "ks", serialize = "ks")]
    Ks,
    #[serde(rename = "ku")]
    #[strum(to_string = "ku", serialize = "ku")]
    Ku,
    #[serde(rename = "kv")]
    #[strum(to_string = "kv", serialize = "kv")]
    Kv,
    #[serde(rename = "kw")]
    #[strum(to_string = "kw", serialize = "kw")]
    Kw,
    #[serde(rename = "ky")]
    #[strum(to_string = "ky", serialize = "ky")]
    Ky,
    #[serde(rename = "la")]
    #[strum(to_string = "la", serialize = "la")]
    La,
    #[serde(rename = "lb")]
    #[strum(to_string = "lb", serialize = "lb")]
    Lb,
    #[serde(rename = "lg")]
    #[strum(to_string = "lg", serialize = "lg")]
    Lg,
    #[serde(rename = "li")]
    #[strum(to_string = "li", serialize = "li")]
    Li,
    #[serde(rename = "ln")]
    #[strum(to_string = "ln", serialize = "ln")]
    Ln,
    #[serde(rename = "lo")]
    #[strum(to_string = "lo", serialize = "lo")]
    Lo,
    #[serde(rename = "lt")]
    #[strum(to_string = "lt", serialize = "lt")]
    Lt,
    #[serde(rename = "lu")]
    #[strum(to_string = "lu", serialize = "lu")]
    Lu,
    #[serde(rename = "lv")]
    #[strum(to_string = "lv", serialize = "lv")]
    Lv,
    #[serde(rename = "mg")]
    #[strum(to_string = "mg", serialize = "mg")]
    Mg,
    #[serde(rename = "mh")]
    #[strum(to_string = "mh", serialize = "mh")]
    Mh,
    #[serde(rename = "mi")]
    #[strum(to_string = "mi", serialize = "mi")]
    Mi,
    #[serde(rename = "mk")]
    #[strum(to_string = "mk", serialize = "mk")]
    Mk,
    #[serde(rename = "ml")]
    #[strum(to_string = "ml", serialize = "ml")]
    Ml,
    #[serde(rename = "mn")]
    #[strum(to_string = "mn", serialize = "mn")]
    Mn,
    #[serde(rename = "mo")]
    #[strum(to_string = "mo", serialize = "mo")]
    Mo,
    #[serde(rename = "mr")]
    #[strum(to_string = "mr", serialize = "mr")]
    Mr,
    #[serde(rename = "ms")]
    #[strum(to_string = "ms", serialize = "ms")]
    Ms,
    #[serde(rename = "mt")]
    #[strum(to_string = "mt", serialize = "mt")]
    Mt,
    #[serde(rename = "my")]
    #[strum(to_string = "my", serialize = "my")]
    My,
    #[serde(rename = "na")]
    #[strum(to_string = "na", serialize = "na")]
    Na,
    #[serde(rename = "nb")]
    #[strum(to_string = "nb", serialize = "nb")]
    Nb,
    #[serde(rename = "nd")]
    #[strum(to_string = "nd", serialize = "nd")]
    Nd,
    #[serde(rename = "ne")]
    #[strum(to_string = "ne", serialize = "ne")]
    Ne,
    #[serde(rename = "ng")]
    #[strum(to_string = "ng", serialize = "ng")]
    Ng,
    #[serde(rename = "nl")]
    #[strum(to_string = "nl", serialize = "nl")]
    Nl,
    #[serde(rename = "nn")]
    #[strum(to_string = "nn", serialize = "nn")]
    Nn,
    #[serde(rename = "no")]
    #[strum(to_string = "no", serialize = "no")]
    No,
    #[serde(rename = "nr")]
    #[strum(to_string = "nr", serialize = "nr")]
    Nr,
    #[serde(rename = "nv")]
    #[strum(to_string = "nv", serialize = "nv")]
    Nv,
    #[serde(rename = "ny")]
    #[strum(to_string = "ny", serialize = "ny")]
    Ny,
    #[serde(rename = "oc")]
    #[strum(to_string = "oc", serialize = "oc")]
    Oc,
    #[serde(rename = "oj")]
    #[strum(to_string = "oj", serialize = "oj")]
    Oj,
    #[serde(rename = "om")]
    #[strum(to_string = "om", serialize = "om")]
    Om,
    #[serde(rename = "or")]
    #[strum(to_string = "or", serialize = "or")]
    Or,
    #[serde(rename = "os")]
    #[strum(to_string = "os", serialize = "os")]
    Os,
    #[serde(rename = "pa")]
    #[strum(to_string = "pa", serialize = "pa")]
    Pa,
    #[serde(rename = "pi")]
    #[strum(to_string = "pi", serialize = "pi")]
    Pi,
    #[serde(rename = "pl")]
    #[strum(to_string = "pl", serialize = "pl")]
    Pl,
    #[serde(rename = "ps")]
    #[strum(to_string = "ps", serialize = "ps")]
    Ps,
    #[serde(rename = "pt")]
    #[strum(to_string = "pt", serialize = "pt")]
    Pt,
    #[serde(rename = "qu")]
    #[strum(to_string = "qu", serialize = "qu")]
    Qu,
    #[serde(rename = "rm")]
    #[strum(to_string = "rm", serialize = "rm")]
    Rm,
    #[serde(rename = "rn")]
    #[strum(to_string = "rn", serialize = "rn")]
    Rn,
    #[serde(rename = "ro")]
    #[strum(to_string = "ro", serialize = "ro")]
    Ro,
    #[serde(rename = "ru")]
    #[strum(to_string = "ru", serialize = "ru")]
    Ru,
    #[serde(rename = "rw")]
    #[strum(to_string = "rw", serialize = "rw")]
    Rw,
    #[serde(rename = "sa")]
    #[strum(to_string = "sa", serialize = "sa")]
    Sa,
    #[serde(rename = "sc")]
    #[strum(to_string = "sc", serialize = "sc")]
    Sc,
    #[serde(rename = "sd")]
    #[strum(to_string = "sd", serialize = "sd")]
    Sd,
    #[serde(rename = "se")]
    #[strum(to_string = "se", serialize = "se")]
    Se,
    #[serde(rename = "sg")]
    #[strum(to_string = "sg", serialize = "sg")]
    Sg,
    #[serde(rename = "sh")]
    #[strum(to_string = "sh", serialize = "sh")]
    Sh,
    #[serde(rename = "si")]
    #[strum(to_string = "si", serialize = "si")]
    Si,
    #[serde(rename = "sk")]
    #[strum(to_string = "sk", serialize = "sk")]
    Sk,
    #[serde(rename = "sl")]
    #[strum(to_string = "sl", serialize = "sl")]
    Sl,
    #[serde(rename = "sm")]
    #[strum(to_string = "sm", serialize = "sm")]
    Sm,
    #[serde(rename = "sn")]
    #[strum(to_string = "sn", serialize = "sn")]
    Sn,
    #[serde(rename = "so")]
    #[strum(to_string = "so", serialize = "so")]
    So,
    #[serde(rename = "sq")]
    #[strum(to_string = "sq", serialize = "sq")]
    Sq,
    #[serde(rename = "sr")]
    #[strum(to_string = "sr", serialize = "sr")]
    Sr,
    #[serde(rename = "ss")]
    #[strum(to_string = "ss", serialize = "ss")]
    Ss,
    #[serde(rename = "st")]
    #[strum(to_string = "st", serialize = "st")]
    St,
    #[serde(rename = "su")]
    #[strum(to_string = "su", serialize = "su")]
    Su,
    #[serde(rename = "sv")]
    #[strum(to_string = "sv", serialize = "sv")]
    Sv,
    #[serde(rename = "sw")]
    #[strum(to_string = "sw", serialize = "sw")]
    Sw,
    #[serde(rename = "ta")]
    #[strum(to_string = "ta", serialize = "ta")]
    Ta,
    #[serde(rename = "te")]
    #[strum(to_string = "te", serialize = "te")]
    Te,
    #[serde(rename = "tg")]
    #[strum(to_string = "tg", serialize = "tg")]
    Tg,
    #[serde(rename = "th")]
    #[strum(to_string = "th", serialize = "th")]
    Th,
    #[serde(rename = "ti")]
    #[strum(to_string = "ti", serialize = "ti")]
    Ti,
    #[serde(rename = "tk")]
    #[strum(to_string = "tk", serialize = "tk")]
    Tk,
    #[serde(rename = "tl")]
    #[strum(to_string = "tl", serialize = "tl")]
    Tl,
    #[serde(rename = "tn")]
    #[strum(to_string = "tn", serialize = "tn")]
    Tn,
    #[serde(rename = "to")]
    #[strum(to_string = "to", serialize = "to")]
    To,
    #[serde(rename = "tr")]
    #[strum(to_string = "tr", serialize = "tr")]
    Tr,
    #[serde(rename = "ts")]
    #[strum(to_string = "ts", serialize = "ts")]
    Ts,
    #[serde(rename = "tt")]
    #[strum(to_string = "tt", serialize = "tt")]
    Tt,
    #[serde(rename = "tw")]
    #[strum(to_string = "tw", serialize = "tw")]
    Tw,
    #[serde(rename = "ty")]
    #[strum(to_string = "ty", serialize = "ty")]
    Ty,
    #[serde(rename = "ug")]
    #[strum(to_string = "ug", serialize = "ug")]
    Ug,
    #[serde(rename = "uk")]
    #[strum(to_string = "uk", serialize = "uk")]
    Uk,
    #[serde(rename = "ur")]
    #[strum(to_string = "ur", serialize = "ur")]
    Ur,
    #[serde(rename = "uz")]
    #[strum(to_string = "uz", serialize = "uz")]
    Uz,
    #[serde(rename = "ve")]
    #[strum(to_string = "ve", serialize = "ve")]
    Ve,
    #[serde(rename = "vi")]
    #[strum(to_string = "vi", serialize = "vi")]
    Vi,
    #[serde(rename = "vo")]
    #[strum(to_string = "vo", serialize = "vo")]
    Vo,
    #[serde(rename = "wa")]
    #[strum(to_string = "wa", serialize = "wa")]
    Wa,
    #[serde(rename = "wo")]
    #[strum(to_string = "wo", serialize = "wo")]
    Wo,
    #[serde(rename = "xh")]
    #[strum(to_string = "xh", serialize = "xh")]
    Xh,
    #[serde(rename = "yi")]
    #[strum(to_string = "yi", serialize = "yi")]
    Yi,
    #[serde(rename = "yo")]
    #[strum(to_string = "yo", serialize = "yo")]
    Yo,
    #[serde(rename = "za")]
    #[strum(to_string = "za", serialize = "za")]
    Za,
    #[serde(rename = "zh")]
    #[strum(to_string = "zh", serialize = "zh")]
    Zh,
    #[serde(rename = "zu")]
    #[strum(to_string = "zu", serialize = "zu")]
    Zu,
}

impl Language {
    pub fn iter() -> <Self as strum::IntoEnumIterator>::Iterator {
        <Self as strum::IntoEnumIterator>::iter()
    }

    pub fn code(&self) -> &'static str {
        serde_variant::to_variant_name(self).unwrap_or_else(|_| self.into())
    }

    pub fn name(&self) -> &'static str {
        // spellcheck:ignore-block
        match self {
            Language::Aa => "Afar",
            Language::Ab => "Abkhazian",
            Language::Af => "Afrikaans",
            Language::Ak => "Akan",
            Language::Am => "Amharic",
            Language::An => "Aragonese",
            Language::Ar => "Arabic",
            Language::As => "Assamese",
            Language::Av => "Avar",
            Language::Ay => "Aymara",
            Language::Az => "Azerbaijani",
            Language::Ba => "Bashkir",
            Language::Be => "Belarusian",
            Language::Bg => "Bulgarian",
            Language::Bh => "Bihari",
            Language::Bi => "Bislama",
            Language::Bm => "Bambara",
            Language::Bn => "Bengali",
            Language::Bo => "Tibetan",
            Language::Br => "Breton",
            Language::Bs => "Bosnian",
            Language::Ca => "Catalan",
            Language::Ce => "Chechen",
            Language::Ch => "Chamorro",
            Language::Co => "Corsican",
            Language::Cr => "Cree",
            Language::Cs => "Czech",
            Language::Cu => "Old Church Slavonic / Old Bulgarian",
            Language::Cv => "Chuvash",
            Language::Cy => "Welsh",
            Language::Da => "Danish",
            Language::De => "German",
            Language::Dv => "Divehi",
            Language::Dz => "Dzongkha",
            Language::Ee => "Ewe",
            Language::El => "Greek",
            Language::En => "English",
            Language::Eo => "Esperanto",
            Language::Es => "Spanish",
            Language::Et => "Estonian",
            Language::Eu => "Basque",
            Language::Fa => "Persian",
            Language::Ff => "Peul",
            Language::Fi => "Finnish",
            Language::Fj => "Fijian",
            Language::Fo => "Faroese",
            Language::Fr => "French",
            Language::Fy => "West Frisian",
            Language::Ga => "Irish",
            Language::Gd => "Scottish Gaelic",
            Language::Gl => "Galician",
            Language::Gn => "Guarani",
            Language::Gu => "Gujarati",
            Language::Gv => "Manx",
            Language::Ha => "Hausa",
            Language::He => "Hebrew",
            Language::Hi => "Hindi",
            Language::Ho => "Hiri Motu",
            Language::Hr => "Croatian",
            Language::Ht => "Haitian",
            Language::Hu => "Hungarian",
            Language::Hy => "Armenian",
            Language::Hz => "Herero",
            Language::Ia => "Interlingua",
            Language::Id => "Indonesian",
            Language::Ie => "Interlingue",
            Language::Ig => "Igbo",
            Language::Ii => "Sichuan Yi",
            Language::Ik => "Inupiak",
            Language::Io => "Ido",
            Language::Is => "Icelandic",
            Language::It => "Italian",
            Language::Iu => "Inuktitut",
            Language::Ja => "Japanese",
            Language::Jv => "Javanese",
            Language::Ka => "Georgian",
            Language::Kg => "Kongo",
            Language::Ki => "Kikuyu",
            Language::Kj => "Kuanyama",
            Language::Kk => "Kazakh",
            Language::Kl => "Greenlandic",
            Language::Km => "Cambodian",
            Language::Kn => "Kannada",
            Language::Ko => "Korean",
            Language::Kr => "Kanuri",
            Language::Ks => "Kashmiri",
            Language::Ku => "Kurdish",
            Language::Kv => "Komi",
            Language::Kw => "Cornish",
            Language::Ky => "Kirghiz",
            Language::La => "Latin",
            Language::Lb => "Luxembourgish",
            Language::Lg => "Ganda",
            Language::Li => "Limburgian",
            Language::Ln => "Lingala",
            Language::Lo => "Laotian",
            Language::Lt => "Lithuanian",
            Language::Lu => "Luba-Katanga",
            Language::Lv => "Latvian",
            Language::Mg => "Malagasy",
            Language::Mh => "Marshallese",
            Language::Mi => "Maori",
            Language::Mk => "Macedonian",
            Language::Ml => "Malayalam",
            Language::Mn => "Mongolian",
            Language::Mo => "Moldovan",
            Language::Mr => "Marathi",
            Language::Ms => "Malay",
            Language::Mt => "Maltese",
            Language::My => "Burmese",
            Language::Na => "Nauruan",
            Language::Nb => "Norwegian Bokmål",
            Language::Nd => "North Ndebele",
            Language::Ne => "Nepali",
            Language::Ng => "Ndonga",
            Language::Nl => "Dutch",
            Language::Nn => "Norwegian Nynorsk",
            Language::No => "Norwegian",
            Language::Nr => "South Ndebele",
            Language::Nv => "Navajo",
            Language::Ny => "Chichewa",
            Language::Oc => "Occitan",
            Language::Oj => "Ojibwa",
            Language::Om => "Oromo",
            Language::Or => "Oriya",
            Language::Os => "Ossetian / Ossetic",
            Language::Pa => "Panjabi / Punjabi",
            Language::Pi => "Pali",
            Language::Pl => "Polish",
            Language::Ps => "Pashto",
            Language::Pt => "Portuguese",
            Language::Qu => "Quechua",
            Language::Rm => "Raeto Romance",
            Language::Rn => "Kirundi",
            Language::Ro => "Romanian",
            Language::Ru => "Russian",
            Language::Rw => "Rwandi",
            Language::Sa => "Sanskrit",
            Language::Sc => "Sardinian",
            Language::Sd => "Sindhi",
            Language::Se => "Northern Sami",
            Language::Sg => "Sango",
            Language::Sh => "Serbo-Croatian",
            Language::Si => "Sinhalese",
            Language::Sk => "Slovak",
            Language::Sl => "Slovenian",
            Language::Sm => "Samoan",
            Language::Sn => "Shona",
            Language::So => "Somalia",
            Language::Sq => "Albanian",
            Language::Sr => "Serbian",
            Language::Ss => "Swati",
            Language::St => "Southern Sotho",
            Language::Su => "Sundanese",
            Language::Sv => "Swedish",
            Language::Sw => "Swahili",
            Language::Ta => "Tamil",
            Language::Te => "Telugu",
            Language::Tg => "Tajik",
            Language::Th => "Thai",
            Language::Ti => "Tigrinya",
            Language::Tk => "Turkmen",
            Language::Tl => "Tagalog / Filipino",
            Language::Tn => "Tswana",
            Language::To => "Tonga",
            Language::Tr => "Turkish",
            Language::Ts => "Tsonga",
            Language::Tt => "Tatar",
            Language::Tw => "Twi",
            Language::Ty => "Tahitian",
            Language::Ug => "Uyghur",
            Language::Uk => "Ukrainian",
            Language::Ur => "Urdu",
            Language::Uz => "Uzbek",
            Language::Ve => "Venda",
            Language::Vi => "Vietnamese",
            Language::Vo => "Volapük",
            Language::Wa => "Walloon",
            Language::Wo => "Wolof",
            Language::Xh => "Xhosa",
            Language::Yi => "Yiddish",
            Language::Yo => "Yoruba",
            Language::Za => "Zhuang",
            Language::Zh => "Chinese",
            Language::Zu => "Zulu",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Language;
    use color_eyre::eyre;
    use similar_asserts::assert_eq as sim_assert_eq;

    #[test]
    fn test_code() -> eyre::Result<()> {
        use std::str::FromStr;
        crate::tests::init();

        for language in Language::iter() {
            sim_assert_eq!(format!("{language}"), language.code());
            sim_assert_eq!(language.to_string(), language.code());
            sim_assert_eq!(Language::try_from(language.code()).ok(), Some(language));
            sim_assert_eq!(Language::from_str(language.code()).ok(), Some(language));
            sim_assert_eq!(
                serde_json::to_value(language)?.as_str(),
                Some(language.code())
            );
        }

        Ok(())
    }
}
