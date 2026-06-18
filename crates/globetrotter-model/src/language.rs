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
    /// Afar.
    #[serde(rename = "aa")]
    #[strum(to_string = "aa")]
    Aa,
    /// Abkhazian.
    #[serde(rename = "ab")]
    #[strum(to_string = "ab")]
    Ab,
    /// Afrikaans.
    #[serde(rename = "af")]
    #[strum(to_string = "af")]
    Af,
    /// Akan.
    #[serde(rename = "ak")]
    #[strum(to_string = "ak")]
    Ak,
    /// Amharic.
    #[serde(rename = "am")]
    #[strum(to_string = "am")]
    Am,
    /// Aragonese.
    #[serde(rename = "an")]
    #[strum(to_string = "an")]
    An,
    /// Arabic.
    #[serde(rename = "ar")]
    #[strum(to_string = "ar")]
    Ar,
    /// Assamese.
    #[serde(rename = "as")]
    #[strum(to_string = "as")]
    As,
    /// Avar.
    #[serde(rename = "av")]
    #[strum(to_string = "av")]
    Av,
    /// Aymara.
    #[serde(rename = "ay")]
    #[strum(to_string = "ay")]
    Ay,
    /// Azerbaijani.
    #[serde(rename = "az")]
    #[strum(to_string = "az")]
    Az,
    /// Bashkir.
    #[serde(rename = "ba")]
    #[strum(to_string = "ba")]
    Ba,
    /// Belarusian.
    #[serde(rename = "be")]
    #[strum(to_string = "be")]
    Be,
    /// Bulgarian.
    #[serde(rename = "bg")]
    #[strum(to_string = "bg")]
    Bg,
    /// Bihari.
    #[serde(rename = "bh")]
    #[strum(to_string = "bh")]
    Bh,
    /// Bislama.
    #[serde(rename = "bi")]
    #[strum(to_string = "bi")]
    Bi,
    /// Bambara.
    #[serde(rename = "bm")]
    #[strum(to_string = "bm")]
    Bm,
    /// Bengali.
    #[serde(rename = "bn")]
    #[strum(to_string = "bn")]
    Bn,
    /// Tibetan.
    #[serde(rename = "bo")]
    #[strum(to_string = "bo")]
    Bo,
    /// Breton.
    #[serde(rename = "br")]
    #[strum(to_string = "br")]
    Br,
    /// Bosnian.
    #[serde(rename = "bs")]
    #[strum(to_string = "bs")]
    Bs,
    /// Catalan.
    #[serde(rename = "ca")]
    #[strum(to_string = "ca")]
    Ca,
    /// Chechen.
    #[serde(rename = "ce")]
    #[strum(to_string = "ce")]
    Ce,
    /// Chamorro.
    #[serde(rename = "ch")]
    #[strum(to_string = "ch")]
    Ch,
    /// Corsican.
    #[serde(rename = "co")]
    #[strum(to_string = "co")]
    Co,
    /// Cree.
    #[serde(rename = "cr")]
    #[strum(to_string = "cr")]
    Cr,
    /// Czech.
    #[serde(rename = "cs")]
    #[strum(to_string = "cs")]
    Cs,
    /// Old Church Slavonic / Old Bulgarian.
    #[serde(rename = "cu")]
    #[strum(to_string = "cu")]
    Cu,
    /// Chuvash.
    #[serde(rename = "cv")]
    #[strum(to_string = "cv")]
    Cv,
    /// Welsh.
    #[serde(rename = "cy")]
    #[strum(to_string = "cy")]
    Cy,
    /// Danish.
    #[serde(rename = "da")]
    #[strum(to_string = "da")]
    Da,
    /// German.
    #[serde(rename = "de")]
    #[strum(to_string = "de")]
    De,
    /// Divehi.
    #[serde(rename = "dv")]
    #[strum(to_string = "dv")]
    Dv,
    /// Dzongkha.
    #[serde(rename = "dz")]
    #[strum(to_string = "dz")]
    Dz,
    /// Ewe.
    #[serde(rename = "ee")]
    #[strum(to_string = "ee")]
    Ee,
    /// Greek.
    #[serde(rename = "el")]
    #[strum(to_string = "el")]
    El,
    /// English.
    #[serde(rename = "en")]
    #[strum(to_string = "en")]
    En,
    /// Esperanto.
    #[serde(rename = "eo")]
    #[strum(to_string = "eo")]
    Eo,
    /// Spanish.
    #[serde(rename = "es")]
    #[strum(to_string = "es")]
    Es,
    /// Estonian.
    #[serde(rename = "et")]
    #[strum(to_string = "et")]
    Et,
    /// Basque.
    #[serde(rename = "eu")]
    #[strum(to_string = "eu")]
    Eu,
    /// Persian.
    #[serde(rename = "fa")]
    #[strum(to_string = "fa")]
    Fa,
    /// Peul.
    #[serde(rename = "ff")]
    #[strum(to_string = "ff")]
    Ff,
    /// Finnish.
    #[serde(rename = "fi")]
    #[strum(to_string = "fi")]
    Fi,
    /// Fijian.
    #[serde(rename = "fj")]
    #[strum(to_string = "fj")]
    Fj,
    /// Faroese.
    #[serde(rename = "fo")]
    #[strum(to_string = "fo")]
    Fo,
    /// French.
    #[serde(rename = "fr")]
    #[strum(to_string = "fr")]
    Fr,
    /// West Frisian.
    #[serde(rename = "fy")]
    #[strum(to_string = "fy")]
    Fy,
    /// Irish.
    #[serde(rename = "ga")]
    #[strum(to_string = "ga")]
    Ga,
    /// Scottish Gaelic.
    #[serde(rename = "gd")]
    #[strum(to_string = "gd")]
    Gd,
    /// Galician.
    #[serde(rename = "gl")]
    #[strum(to_string = "gl")]
    Gl,
    /// Guarani.
    #[serde(rename = "gn")]
    #[strum(to_string = "gn")]
    Gn,
    /// Gujarati.
    #[serde(rename = "gu")]
    #[strum(to_string = "gu")]
    Gu,
    /// Manx.
    #[serde(rename = "gv")]
    #[strum(to_string = "gv")]
    Gv,
    /// Hausa.
    #[serde(rename = "ha")]
    #[strum(to_string = "ha")]
    Ha,
    /// Hebrew.
    #[serde(rename = "he")]
    #[strum(to_string = "he")]
    He,
    /// Hindi.
    #[serde(rename = "hi")]
    #[strum(to_string = "hi")]
    Hi,
    /// Hiri Motu.
    #[serde(rename = "ho")]
    #[strum(to_string = "ho")]
    Ho,
    /// Croatian.
    #[serde(rename = "hr")]
    #[strum(to_string = "hr")]
    Hr,
    /// Haitian.
    #[serde(rename = "ht")]
    #[strum(to_string = "ht")]
    Ht,
    /// Hungarian.
    #[serde(rename = "hu")]
    #[strum(to_string = "hu")]
    Hu,
    /// Armenian.
    #[serde(rename = "hy")]
    #[strum(to_string = "hy")]
    Hy,
    /// Herero.
    #[serde(rename = "hz")]
    #[strum(to_string = "hz")]
    Hz,
    /// Interlingua.
    #[serde(rename = "ia")]
    #[strum(to_string = "ia")]
    Ia,
    /// Indonesian.
    #[serde(rename = "id")]
    #[strum(to_string = "id")]
    Id,
    /// Interlingue.
    #[serde(rename = "ie")]
    #[strum(to_string = "ie")]
    Ie,
    /// Igbo.
    #[serde(rename = "ig")]
    #[strum(to_string = "ig")]
    Ig,
    /// Sichuan Yi.
    #[serde(rename = "ii")]
    #[strum(to_string = "ii")]
    Ii,
    /// Inupiak.
    #[serde(rename = "ik")]
    #[strum(to_string = "ik")]
    Ik,
    /// Ido.
    #[serde(rename = "io")]
    #[strum(to_string = "io")]
    Io,
    /// Icelandic.
    #[serde(rename = "is")]
    #[strum(to_string = "is")]
    Is,
    /// Italian.
    #[serde(rename = "it")]
    #[strum(to_string = "it")]
    It,
    /// Inuktitut.
    #[serde(rename = "iu")]
    #[strum(to_string = "iu")]
    Iu,
    /// Japanese.
    #[serde(rename = "ja")]
    #[strum(to_string = "ja")]
    Ja,
    /// Javanese.
    #[serde(rename = "jv")]
    #[strum(to_string = "jv")]
    Jv,
    /// Georgian.
    #[serde(rename = "ka")]
    #[strum(to_string = "ka")]
    Ka,
    /// Kongo.
    #[serde(rename = "kg")]
    #[strum(to_string = "kg")]
    Kg,
    /// Kikuyu.
    #[serde(rename = "ki")]
    #[strum(to_string = "ki")]
    Ki,
    /// Kuanyama.
    #[serde(rename = "kj")]
    #[strum(to_string = "kj")]
    Kj,
    /// Kazakh.
    #[serde(rename = "kk")]
    #[strum(to_string = "kk")]
    Kk,
    /// Greenlandic.
    #[serde(rename = "kl")]
    #[strum(to_string = "kl")]
    Kl,
    /// Cambodian.
    #[serde(rename = "km")]
    #[strum(to_string = "km")]
    Km,
    /// Kannada.
    #[serde(rename = "kn")]
    #[strum(to_string = "kn")]
    Kn,
    /// Korean.
    #[serde(rename = "ko")]
    #[strum(to_string = "ko")]
    Ko,
    /// Kanuri.
    #[serde(rename = "kr")]
    #[strum(to_string = "kr")]
    Kr,
    /// Kashmiri.
    #[serde(rename = "ks")]
    #[strum(to_string = "ks")]
    Ks,
    /// Kurdish.
    #[serde(rename = "ku")]
    #[strum(to_string = "ku")]
    Ku,
    /// Komi.
    #[serde(rename = "kv")]
    #[strum(to_string = "kv")]
    Kv,
    /// Cornish.
    #[serde(rename = "kw")]
    #[strum(to_string = "kw")]
    Kw,
    /// Kirghiz.
    #[serde(rename = "ky")]
    #[strum(to_string = "ky")]
    Ky,
    /// Latin.
    #[serde(rename = "la")]
    #[strum(to_string = "la")]
    La,
    /// Luxembourgish.
    #[serde(rename = "lb")]
    #[strum(to_string = "lb")]
    Lb,
    /// Ganda.
    #[serde(rename = "lg")]
    #[strum(to_string = "lg")]
    Lg,
    /// Limburgian.
    #[serde(rename = "li")]
    #[strum(to_string = "li")]
    Li,
    /// Lingala.
    #[serde(rename = "ln")]
    #[strum(to_string = "ln")]
    Ln,
    /// Laotian.
    #[serde(rename = "lo")]
    #[strum(to_string = "lo")]
    Lo,
    /// Lithuanian.
    #[serde(rename = "lt")]
    #[strum(to_string = "lt")]
    Lt,
    /// Luba-Katanga.
    #[serde(rename = "lu")]
    #[strum(to_string = "lu")]
    Lu,
    /// Latvian.
    #[serde(rename = "lv")]
    #[strum(to_string = "lv")]
    Lv,
    /// Malagasy.
    #[serde(rename = "mg")]
    #[strum(to_string = "mg")]
    Mg,
    /// Marshallese.
    #[serde(rename = "mh")]
    #[strum(to_string = "mh")]
    Mh,
    /// Maori.
    #[serde(rename = "mi")]
    #[strum(to_string = "mi")]
    Mi,
    /// Macedonian.
    #[serde(rename = "mk")]
    #[strum(to_string = "mk")]
    Mk,
    /// Malayalam.
    #[serde(rename = "ml")]
    #[strum(to_string = "ml")]
    Ml,
    /// Mongolian.
    #[serde(rename = "mn")]
    #[strum(to_string = "mn")]
    Mn,
    /// Moldovan.
    #[serde(rename = "mo")]
    #[strum(to_string = "mo")]
    Mo,
    /// Marathi.
    #[serde(rename = "mr")]
    #[strum(to_string = "mr")]
    Mr,
    /// Malay.
    #[serde(rename = "ms")]
    #[strum(to_string = "ms")]
    Ms,
    /// Maltese.
    #[serde(rename = "mt")]
    #[strum(to_string = "mt")]
    Mt,
    /// Burmese.
    #[serde(rename = "my")]
    #[strum(to_string = "my")]
    My,
    /// Nauruan.
    #[serde(rename = "na")]
    #[strum(to_string = "na")]
    Na,
    /// Norwegian Bokmål.
    #[serde(rename = "nb")]
    #[strum(to_string = "nb")]
    Nb,
    /// North Ndebele.
    #[serde(rename = "nd")]
    #[strum(to_string = "nd")]
    Nd,
    /// Nepali.
    #[serde(rename = "ne")]
    #[strum(to_string = "ne")]
    Ne,
    /// Ndonga.
    #[serde(rename = "ng")]
    #[strum(to_string = "ng")]
    Ng,
    /// Dutch.
    #[serde(rename = "nl")]
    #[strum(to_string = "nl")]
    Nl,
    /// Norwegian Nynorsk.
    #[serde(rename = "nn")]
    #[strum(to_string = "nn")]
    Nn,
    /// Norwegian.
    #[serde(rename = "no")]
    #[strum(to_string = "no")]
    No,
    /// South Ndebele.
    #[serde(rename = "nr")]
    #[strum(to_string = "nr")]
    Nr,
    /// Navajo.
    #[serde(rename = "nv")]
    #[strum(to_string = "nv")]
    Nv,
    /// Chichewa.
    #[serde(rename = "ny")]
    #[strum(to_string = "ny")]
    Ny,
    /// Occitan.
    #[serde(rename = "oc")]
    #[strum(to_string = "oc")]
    Oc,
    /// Ojibwa.
    #[serde(rename = "oj")]
    #[strum(to_string = "oj")]
    Oj,
    /// Oromo.
    #[serde(rename = "om")]
    #[strum(to_string = "om")]
    Om,
    /// Oriya.
    #[serde(rename = "or")]
    #[strum(to_string = "or")]
    Or,
    /// Ossetian / Ossetic.
    #[serde(rename = "os")]
    #[strum(to_string = "os")]
    Os,
    /// Panjabi / Punjabi.
    #[serde(rename = "pa")]
    #[strum(to_string = "pa")]
    Pa,
    /// Pali.
    #[serde(rename = "pi")]
    #[strum(to_string = "pi")]
    Pi,
    /// Polish.
    #[serde(rename = "pl")]
    #[strum(to_string = "pl")]
    Pl,
    /// Pashto.
    #[serde(rename = "ps")]
    #[strum(to_string = "ps")]
    Ps,
    /// Portuguese.
    #[serde(rename = "pt")]
    #[strum(to_string = "pt")]
    Pt,
    /// Quechua.
    #[serde(rename = "qu")]
    #[strum(to_string = "qu")]
    Qu,
    /// Raeto Romance.
    #[serde(rename = "rm")]
    #[strum(to_string = "rm")]
    Rm,
    /// Kirundi.
    #[serde(rename = "rn")]
    #[strum(to_string = "rn")]
    Rn,
    /// Romanian.
    #[serde(rename = "ro")]
    #[strum(to_string = "ro")]
    Ro,
    /// Russian.
    #[serde(rename = "ru")]
    #[strum(to_string = "ru")]
    Ru,
    /// Rwandi.
    #[serde(rename = "rw")]
    #[strum(to_string = "rw")]
    Rw,
    /// Sanskrit.
    #[serde(rename = "sa")]
    #[strum(to_string = "sa")]
    Sa,
    /// Sardinian.
    #[serde(rename = "sc")]
    #[strum(to_string = "sc")]
    Sc,
    /// Sindhi.
    #[serde(rename = "sd")]
    #[strum(to_string = "sd")]
    Sd,
    /// Northern Sami.
    #[serde(rename = "se")]
    #[strum(to_string = "se")]
    Se,
    /// Sango.
    #[serde(rename = "sg")]
    #[strum(to_string = "sg")]
    Sg,
    /// Serbo-Croatian.
    #[serde(rename = "sh")]
    #[strum(to_string = "sh")]
    Sh,
    /// Sinhalese.
    #[serde(rename = "si")]
    #[strum(to_string = "si")]
    Si,
    /// Slovak.
    #[serde(rename = "sk")]
    #[strum(to_string = "sk")]
    Sk,
    /// Slovenian.
    #[serde(rename = "sl")]
    #[strum(to_string = "sl")]
    Sl,
    /// Samoan.
    #[serde(rename = "sm")]
    #[strum(to_string = "sm")]
    Sm,
    /// Shona.
    #[serde(rename = "sn")]
    #[strum(to_string = "sn")]
    Sn,
    /// Somalia.
    #[serde(rename = "so")]
    #[strum(to_string = "so")]
    So,
    /// Albanian.
    #[serde(rename = "sq")]
    #[strum(to_string = "sq")]
    Sq,
    /// Serbian.
    #[serde(rename = "sr")]
    #[strum(to_string = "sr")]
    Sr,
    /// Swati.
    #[serde(rename = "ss")]
    #[strum(to_string = "ss")]
    Ss,
    /// Southern Sotho.
    #[serde(rename = "st")]
    #[strum(to_string = "st")]
    St,
    /// Sundanese.
    #[serde(rename = "su")]
    #[strum(to_string = "su")]
    Su,
    /// Swedish.
    #[serde(rename = "sv")]
    #[strum(to_string = "sv")]
    Sv,
    /// Swahili.
    #[serde(rename = "sw")]
    #[strum(to_string = "sw")]
    Sw,
    /// Tamil.
    #[serde(rename = "ta")]
    #[strum(to_string = "ta")]
    Ta,
    /// Telugu.
    #[serde(rename = "te")]
    #[strum(to_string = "te")]
    Te,
    /// Tajik.
    #[serde(rename = "tg")]
    #[strum(to_string = "tg")]
    Tg,
    /// Thai.
    #[serde(rename = "th")]
    #[strum(to_string = "th")]
    Th,
    /// Tigrinya.
    #[serde(rename = "ti")]
    #[strum(to_string = "ti")]
    Ti,
    /// Turkmen.
    #[serde(rename = "tk")]
    #[strum(to_string = "tk")]
    Tk,
    /// Tagalog / Filipino.
    #[serde(rename = "tl")]
    #[strum(to_string = "tl")]
    Tl,
    /// Tswana.
    #[serde(rename = "tn")]
    #[strum(to_string = "tn")]
    Tn,
    /// Tonga.
    #[serde(rename = "to")]
    #[strum(to_string = "to")]
    To,
    /// Turkish.
    #[serde(rename = "tr")]
    #[strum(to_string = "tr")]
    Tr,
    /// Tsonga.
    #[serde(rename = "ts")]
    #[strum(to_string = "ts")]
    Ts,
    /// Tatar.
    #[serde(rename = "tt")]
    #[strum(to_string = "tt")]
    Tt,
    /// Twi.
    #[serde(rename = "tw")]
    #[strum(to_string = "tw")]
    Tw,
    /// Tahitian.
    #[serde(rename = "ty")]
    #[strum(to_string = "ty")]
    Ty,
    /// Uyghur.
    #[serde(rename = "ug")]
    #[strum(to_string = "ug")]
    Ug,
    /// Ukrainian.
    #[serde(rename = "uk")]
    #[strum(to_string = "uk")]
    Uk,
    /// Urdu.
    #[serde(rename = "ur")]
    #[strum(to_string = "ur")]
    Ur,
    /// Uzbek.
    #[serde(rename = "uz")]
    #[strum(to_string = "uz")]
    Uz,
    /// Venda.
    #[serde(rename = "ve")]
    #[strum(to_string = "ve")]
    Ve,
    /// Vietnamese.
    #[serde(rename = "vi")]
    #[strum(to_string = "vi")]
    Vi,
    /// Volapük.
    #[serde(rename = "vo")]
    #[strum(to_string = "vo")]
    Vo,
    /// Walloon.
    #[serde(rename = "wa")]
    #[strum(to_string = "wa")]
    Wa,
    /// Wolof.
    #[serde(rename = "wo")]
    #[strum(to_string = "wo")]
    Wo,
    /// Xhosa.
    #[serde(rename = "xh")]
    #[strum(to_string = "xh")]
    Xh,
    /// Yiddish.
    #[serde(rename = "yi")]
    #[strum(to_string = "yi")]
    Yi,
    /// Yoruba.
    #[serde(rename = "yo")]
    #[strum(to_string = "yo")]
    Yo,
    /// Zhuang.
    #[serde(rename = "za")]
    #[strum(to_string = "za")]
    Za,
    /// Chinese.
    #[serde(rename = "zh")]
    #[strum(to_string = "zh")]
    Zh,
    /// Zulu.
    #[serde(rename = "zu")]
    #[strum(to_string = "zu")]
    Zu,
}

impl Language {
    /// Iterate over all known languages.
    #[must_use]
    pub fn iter() -> <Self as strum::IntoEnumIterator>::Iterator {
        <Self as strum::IntoEnumIterator>::iter()
    }

    /// Return the ISO 639-1 Alpha-2 code for this language (e.g. `"en"`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        serde_variant::to_variant_name(self).unwrap_or_else(|_| self.into())
    }

    /// Return the English display name for this language (e.g. `"English"`).
    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive ISO 639-1 lookup table; one match arm per language is the clearest form"
    )]
    #[must_use]
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
            sim_assert_eq!(have: format!("{language}"), want: language.code());
            sim_assert_eq!(have: language.to_string(), want: language.code());
            sim_assert_eq!(have: Language::try_from(language.code()).ok(), want: Some(language));
            sim_assert_eq!(have: Language::from_str(language.code()).ok(), want: Some(language));
            sim_assert_eq!(
                have: serde_json::to_value(language)?.as_str(),
                want: Some(language.code())
            );
        }

        Ok(())
    }
}
