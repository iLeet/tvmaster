//! The sounds an alert can start with, made by FFmpeg's own tone generator, so
//! there are no audio files to ship. A sound plays once when the alert goes on air.

/// What an alert sounds like when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertSound {
    /// Silent: a quiet notice, not a jump scare.
    None,
    /// The classic two-tone attention signal (853 and 960 Hz together), eight seconds.
    #[default]
    Attention,
    /// A soft two-note chime, for information.
    Information,
    /// A warbling siren, six seconds, for something urgent.
    Urgent,
    /// Three short beeps.
    Beeps,
}

impl AlertSound {
    pub const ALL: [(&'static str, &'static str); 5] = [
        ("none", "No sound"),
        ("attention", "Attention tone (two-tone)"),
        ("information", "Information chime"),
        ("urgent", "Urgent warble"),
        ("beeps", "Three beeps"),
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "attention" => Self::Attention,
            "information" => Self::Information,
            "urgent" => Self::Urgent,
            "beeps" => Self::Beeps,
            _ => return None,
        })
    }

    /// How long the sound lasts from the moment the alert goes on air.
    pub fn duration_ms(self) -> i64 {
        match self {
            Self::None => 0,
            Self::Attention => 8_000,
            Self::Information => 1_300,
            Self::Urgent => 6_000,
            Self::Beeps => 2_400,
        }
    }

    /// The amplitude as a function of `T`, the seconds since the alert began.
    fn expr(self) -> &'static str {
        match self {
            Self::None => "0",
            Self::Attention => "if(lt(T,8),0.30*sin(2*PI*853*T)+0.30*sin(2*PI*960*T),0)",
            Self::Information => {
                "if(lt(T,0.4),0.35*sin(2*PI*784*T)*exp(-4*T),if(lt(T,1.3),0.35*sin(2*PI*988*T)*exp(-4*(T-0.4)),0))"
            }
            Self::Urgent => "if(lt(T,6),0.30*sin(2*PI*if(lt(mod(T,1),0.5),960,770)*T),0)",
            Self::Beeps => "if(lt(T,2.4),if(lt(mod(T,0.8),0.25),0.32*sin(2*PI*1000*T),0),0)",
        }
    }

    /// An FFmpeg `lavfi` audio source that plays what is left of the sound `into_ms` after the alert
    /// began (so a piece of it can be added to each stretch of picture), or `None` when it is over.
    pub fn source(self, into_ms: i64) -> Option<String> {
        if self == Self::None || into_ms >= self.duration_ms() {
            return None;
        }
        // T is the alert's own clock; `t` the source's, which starts at zero.
        let e = self
            .expr()
            .replace('T', &format!("(t+{:.3})", into_ms.max(0) as f64 / 1000.0))
            .replace(',', "\\,");
        Some(format!("aevalsrc={e}|{e}:s=48000"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_is_over_after_its_length_and_none_is_silent() {
        assert!(AlertSound::None.source(0).is_none());
        for (name, _) in AlertSound::ALL {
            let s = AlertSound::parse(name).unwrap();
            assert_eq!(s.source(s.duration_ms()), None, "{name} ends");
            if s != AlertSound::None {
                let src = s.source(500).unwrap();
                assert!(
                    src.starts_with("aevalsrc=") && src.contains("(t+0.500)"),
                    "{src}"
                );
            }
        }
        assert!(AlertSound::parse("loud").is_none());
    }
}
