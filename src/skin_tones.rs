/// Call `$then!` with every skin tone, the default first
macro_rules! with_skin_tones {
    ($then:ident) => {
        $then! {
            Default,
            Light,
            MediumLight,
            Medium,
            MediumDark,
            Dark,
            LightAndMediumLight,
            LightAndMedium,
            LightAndMediumDark,
            LightAndDark,
            MediumLightAndLight,
            MediumLightAndMedium,
            MediumLightAndMediumDark,
            MediumLightAndDark,
            MediumAndLight,
            MediumAndMediumLight,
            MediumAndMediumDark,
            MediumAndDark,
            MediumDarkAndLight,
            MediumDarkAndMediumLight,
            MediumDarkAndMedium,
            MediumDarkAndDark,
            DarkAndLight,
            DarkAndMediumLight,
            DarkAndMedium,
            DarkAndMediumDark,
        }
    };
}
