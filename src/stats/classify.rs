use std::{net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::http::{HeaderMap, HeaderName, header};

use crate::config::Config;

#[derive(Debug, PartialEq)]
pub struct ClickInfo {
    pub referrer: String,
    pub country: String,
    pub device: &'static str,
    pub browser: String,
    pub os: String,
}

pub struct Classifier {
    ua: woothee::parser::Parser,
    /// Parsing is most of a redirect's cost and user agents repeat; `None` is a bot.
    agents: moka::sync::Cache<String, Option<Arc<Agent>>>,
    geoip: Option<maxminddb::Reader<Vec<u8>>>,
    /// `CF-IPCountry` is only Cloudflare's when the client IP comes from it too; otherwise any client sets it.
    cf_country: bool,
}

#[derive(Debug)]
struct Agent {
    device: &'static str,
    browser: String,
    os: String,
}

const BOTS: &[&str] = &[
    "slackbot",
    "twitterbot",
    "facebookexternalhit",
    "whatsapp",
    "discordbot",
    "telegrambot",
    "linkedinbot",
    "googlebot",
    "bingbot",
];

impl Classifier {
    pub fn new(config: &Config) -> anyhow::Result<Classifier> {
        let geoip = config
            .geoip_db_path
            .as_ref()
            .map(|p| {
                maxminddb::Reader::open_readfile(p)
                    .with_context(|| format!("opening geoip db {}", p.display()))
            })
            .transpose()?;
        Ok(Classifier {
            ua: woothee::parser::Parser::new(),
            agents: moka::sync::Cache::new(10_000),
            geoip,
            cf_country: matches!(
                config.client_ip_source,
                axum_client_ip::ClientIpSource::CfConnectingIp
            ),
        })
    }

    pub fn classify(&self, headers: &HeaderMap, client_ip: Option<IpAddr>) -> Option<ClickInfo> {
        let header = |h| headers.get(h).and_then(|v| v.to_str().ok());
        // Real user agents are a few hundred bytes; the cap bounds the cache (10k keys) whatever clients send.
        let ua_str = truncate(header(header::USER_AGENT).unwrap_or_default(), 512);
        let agent = match self.agents.get(ua_str) {
            Some(a) => a,
            None => {
                let a = self.agent(ua_str).map(Arc::new);
                self.agents.insert(ua_str.to_owned(), a.clone());
                a
            }
        }?;
        let referrer = header(header::REFERER)
            .and_then(|r| url::Url::parse(r).ok())
            .and_then(|u| {
                u.host_str()
                    .map(|h| h.chars().take(253).collect::<String>().to_ascii_lowercase())
            })
            .unwrap_or_else(|| "direct".into());
        let country = header(HeaderName::from_static("cf-ipcountry"))
            .filter(|_| self.cf_country)
            .filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic()))
            .map(str::to_ascii_uppercase)
            .filter(|c| c != "XX" && c != "T1")
            .or_else(|| self.geo_country(client_ip?))
            .unwrap_or_else(|| "unknown".into());
        Some(ClickInfo {
            referrer,
            country,
            device: agent.device,
            browser: agent.browser.clone(),
            os: agent.os.clone(),
        })
    }

    fn agent(&self, ua_str: &str) -> Option<Agent> {
        let lower = ua_str.to_ascii_lowercase();
        if BOTS.iter().any(|b| lower.contains(b)) {
            return None;
        }
        let ua = self.ua.parse(ua_str).unwrap_or_default();
        if ua.category == "crawler" {
            return None;
        }
        let os = match ua.os {
            "iPhone" | "iPad" | "iPod" => "iOS",
            "Mac OSX" => "macOS",
            o if o.starts_with("Windows") => "Windows",
            "" | woothee::woothee::VALUE_UNKNOWN => "unknown",
            o => o,
        };
        let device = match ua.category {
            _ if ua.os == "iPad" || (ua.os == "Android" && !ua_str.contains("Mobile")) => "tablet",
            "smartphone" | "mobilephone" => "mobile",
            "pc" => "desktop",
            _ => "other",
        };
        let browser = match ua.name {
            "" | woothee::woothee::VALUE_UNKNOWN => "unknown",
            b => b,
        };
        Some(Agent {
            device,
            browser: browser.into(),
            os: os.into(),
        })
    }

    fn geo_country(&self, ip: IpAddr) -> Option<String> {
        let rec = self.geoip.as_ref()?.lookup(ip).ok()?;
        let c = rec.decode::<maxminddb::geoip2::Country>().ok()??;
        c.country.iso_code.map(Into::into)
    }
}

fn truncate(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
    const WIN_CHROME: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    fn classifier() -> Classifier {
        let c = Config::from_vars([
            ("TIINY_BASE_URL".into(), "https://link.example".into()),
            ("TIINY_CLIENT_IP_HEADER".into(), "cf-connecting-ip".into()),
        ])
        .unwrap();
        Classifier::new(&c).unwrap()
    }

    fn run(pairs: &[(&'static str, &str)]) -> Option<ClickInfo> {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        classifier().classify(&h, Some("1.2.3.4".parse().unwrap()))
    }

    fn info(pairs: &[(&'static str, &str)]) -> ClickInfo {
        let mut p = vec![("user-agent", WIN_CHROME)];
        p.extend_from_slice(pairs);
        run(&p).unwrap()
    }

    #[test]
    fn referrer() {
        assert_eq!(
            info(&[("referer", "https://www.google.com/search?q=x")]).referrer,
            "www.google.com"
        );
        assert_eq!(info(&[]).referrer, "direct");
        assert_eq!(info(&[("referer", "::::")]).referrer, "direct");
        let long = format!("https://{}.com/", vec!["a".repeat(60); 6].join("."));
        assert_eq!(info(&[("referer", &long)]).referrer.len(), 253);
    }

    #[test]
    fn country() {
        assert_eq!(info(&[("cf-ipcountry", "IN")]).country, "IN");
        assert_eq!(info(&[]).country, "unknown");
        assert_eq!(info(&[("cf-ipcountry", "XX")]).country, "unknown");
        assert_eq!(info(&[("cf-ipcountry", "T1")]).country, "unknown");
    }

    #[test]
    fn long_user_agents_are_cut_on_a_char_boundary() {
        assert_eq!(truncate("abc", 512), "abc");
        assert_eq!(truncate("ééé", 3), "é");
        let long = "x".repeat(70_000);
        assert_eq!(truncate(&long, 512).len(), 512);
    }

    #[test]
    fn cf_country_ignored_unless_behind_cloudflare() {
        let c =
            Config::from_vars([("TIINY_BASE_URL".into(), "https://link.example".into())]).unwrap();
        let mut h = HeaderMap::new();
        h.insert(header::USER_AGENT, WIN_CHROME.parse().unwrap());
        h.insert("cf-ipcountry", "KP".parse().unwrap());
        let info = Classifier::new(&c).unwrap().classify(&h, None).unwrap();
        assert_eq!(info.country, "unknown");
    }

    #[test]
    fn user_agent() {
        let i = run(&[("user-agent", IPHONE)]).unwrap();
        assert_eq!((i.device, &*i.browser, &*i.os), ("mobile", "Safari", "iOS"));
        let i = info(&[]);
        assert_eq!(
            (i.device, &*i.browser, &*i.os),
            ("desktop", "Chrome", "Windows")
        );
    }

    #[test]
    fn bots() {
        for b in [
            "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)",
            "Twitterbot/1.0",
            "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
            "WhatsApp/2.23.20.0",
            "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)",
            "TelegramBot (like TwitterBot)",
            "LinkedInBot/1.0 (compatible; Mozilla/5.0; Apache-HttpClient +http://www.linkedin.com)",
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
        ] {
            assert_eq!(run(&[("user-agent", b)]), None, "{b}");
        }
    }
}
