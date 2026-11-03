//! Débrief par IA (« ingénieur de piste ») : conversation en langage naturel sur une séance.
//!
//! Principe : l'IA ne voit jamais la télémétrie brute. Apex lui envoie la synthèse chiffrée qu'il a
//! calculée (tours, écarts par virage, conseils, carte de risque, régularité, course) et une consigne
//! stricte : répondre uniquement à partir de ces chiffres, sans en inventer. Elle reformule, relie et
//! explique ; les mesures restent celles d'Apex.
//!
//! Service : API Claude (clé personnelle du pilote, facturée par Anthropic à l'usage).

use serde::{Deserialize, Serialize};
use serde_json::json;

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// Taille maximale de la conversation conservée (messages), pour borner le coût d'une question.
const MAX_TURNS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// « user » ou « assistant ».
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatReply {
    pub text: String,
    /// Jetons en entrée facturés plein tarif (dont écriture en cache), lus depuis le cache, et en sortie.
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    /// Coût indicatif de la réponse (USD).
    pub cost_usd: f64,
}

/// Consignes de l'ingénieur de piste.
pub fn system_prompt(beginner: bool) -> String {
    let mut s = String::from(
        "Tu es l'ingénieur de piste d'un pilote de simracing, dans l'application Apex Companion. \
Tu réponds en français, tutoiement, ton direct et bienveillant, comme à la radio d'un stand.\n\n\
Règles absolues :\n\
- Tu t'appuies UNIQUEMENT sur les données de séance fournies (bloc DONNÉES). Elles sont mesurées par Apex : ce sont les seuls faits.\n\
- Tu n'inventes jamais un chiffre, un virage, un point de freinage ou une cause. Si l'information n'est pas dans les données, dis-le simplement (« je n'ai pas cette mesure »).\n\
- Quand tu cites un chiffre, reprends-le tel quel depuis les données.\n\
- Les conseils de technique doivent découler des écarts mesurés (freinage, vitesse mini, remise des gaz, vitesse de sortie, sorties de piste). Tu peux expliquer le pourquoi en termes de pilotage général, sans affirmer ce que les données ne montrent pas.\n\
- Pas de conseils de réglages (setup) : tu n'as pas ces données.\n\
- Réponses courtes : 3 à 8 lignes, une priorité claire. Pas de tableau sauf si on te le demande.\n",
    );
    if beginner {
        s += "- Le pilote est débutant : vocabulaire simple, une seule chose à travailler à la fois, la régularité et rester sur la piste avant la vitesse pure.\n";
    }
    s
}

/// Message envoyé à l'API : données de séance en tête de la première question, puis l'échange.
pub fn build_request(model: &str, system: &str, context: &str, history: &[ChatMessage]) -> serde_json::Value {
    let start = history.len().saturating_sub(MAX_TURNS);
    let mut msgs: Vec<serde_json::Value> = vec![];
    for (i, m) in history[start..].iter().enumerate() {
        let role = if m.role == "assistant" { "assistant" } else { "user" };
        if i == 0 && role == "user" {
            // Données mises en cache côté API : les questions suivantes de la conversation coûtent ~10 fois moins.
            msgs.push(json!({ "role": "user", "content": [
                { "type": "text", "text": format!("DONNÉES DE LA SÉANCE (mesurées par Apex) :\n{context}"), "cache_control": { "type": "ephemeral" } },
                { "type": "text", "text": m.text },
            ]}));
        } else {
            msgs.push(json!({ "role": role, "content": m.text }));
        }
    }
    // L'API exige un premier message « user » : une conversation tronquée qui commencerait par
    // l'assistant est complétée par les données seules.
    if msgs.first().is_some_and(|m| m["role"] == "assistant") {
        msgs.insert(0, json!({ "role": "user", "content": format!("DONNÉES DE LA SÉANCE (mesurées par Apex) :\n{context}") }));
    }
    json!({ "model": model, "max_tokens": 1500, "system": system, "messages": msgs })
}

/// Texte de la réponse (blocs « text » seulement ; la réflexion éventuelle est ignorée).
pub fn parse_reply(v: &serde_json::Value) -> Result<ChatReply, String> {
    if let Some(e) = v.get("error") {
        return Err(e["message"].as_str().unwrap_or("erreur de l'API").to_string());
    }
    let text: String = v["content"]
        .as_array()
        .ok_or("réponse de l'IA illisible")?
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err("réponse vide de l'IA".into());
    }
    let u = &v["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let (input, cached, output) = (n("input_tokens") + n("cache_creation_input_tokens"), n("cache_read_input_tokens"), n("output_tokens"));
    Ok(ChatReply { text, input_tokens: input, cached_tokens: cached, output_tokens: output, cost_usd: approx_cost_usd(input, cached, output) })
}

/// Appel de l'API Claude.
pub fn ask(api_key: &str, body: &serde_json::Value) -> Result<ChatReply, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(90))).http_status_as_error(false).build().into();
    let mut resp = agent
        .post(API_URL)
        .header("x-api-key", api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|e| format!("connexion à l'API impossible ({e})"))?;
    let status = resp.status().as_u16();
    let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| format!("réponse illisible ({e})"))?;
    match status {
        200 => parse_reply(&v),
        401 => Err("clé API refusée : vérifie-la dans Réglages → Débrief IA".into()),
        429 => Err("trop de demandes ou crédit épuisé sur ton compte API : réessaie dans un moment".into()),
        _ => Err(v["error"]["message"].as_str().map(|m| format!("API ({status}) : {m}")).unwrap_or_else(|| format!("API : erreur {status}"))),
    }
}

/// Coût approximatif (USD), pour l'afficher au pilote. Tarif public de Claude Sonnet 5.5 (novembre 2026) :
/// 2 $ / million de jetons en entrée, 10 $ en sortie, lecture du cache à 5 % du prix d'entrée ; indicatif.
pub fn approx_cost_usd(input: u64, cached: u64, output: u64) -> f64 {
    input as f64 * 2.0 / 1e6 + cached as f64 * 0.1 / 1e6 + output as f64 * 10.0 / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: &str, t: &str) -> ChatMessage {
        ChatMessage { role: role.into(), text: t.into() }
    }

    #[test]
    fn data_ride_with_the_first_question_and_are_cached() {
        let b = build_request("claude-sonnet-5-5", "sys", "Tour 3 : 1:51.2", &[m("user", "Où je perds ?"), m("assistant", "Au V7."), m("user", "Pourquoi ?")]);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "user");
        assert!(msgs[0]["content"][0]["text"].as_str().unwrap().contains("Tour 3 : 1:51.2"));
        assert_eq!(msgs[0]["content"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(msgs[0]["content"][1]["text"], "Où je perds ?");
        assert_eq!(msgs[2]["content"], "Pourquoi ?");
        assert!(b.get("temperature").is_none(), "paramètre refusé par les modèles récents");
    }

    #[test]
    fn long_conversations_are_trimmed_but_keep_the_data() {
        let mut h = vec![];
        for i in 0..40 {
            h.push(m(if i % 2 == 0 { "user" } else { "assistant" }, &format!("msg {i}")));
        }
        h.push(m("user", "dernière"));
        let b = build_request("x", "s", "DATA", &h);
        let msgs = b["messages"].as_array().unwrap();
        assert!(msgs.len() <= MAX_TURNS + 1);
        assert_eq!(msgs[0]["role"], "user");
        assert!(serde_json::to_string(&msgs[0]).unwrap().contains("DATA"));
        assert_eq!(msgs.last().unwrap()["content"], "dernière");
    }

    #[test]
    fn reply_text_ignores_thinking_and_counts_tokens() {
        let v = json!({ "content": [ { "type": "thinking", "thinking": "..." }, { "type": "text", "text": "Freine 10 m plus tard au V7." } ],
            "usage": { "input_tokens": 100, "cache_read_input_tokens": 9000, "output_tokens": 50 } });
        let r = parse_reply(&v).unwrap();
        assert_eq!(r.text, "Freine 10 m plus tard au V7.");
        assert_eq!((r.input_tokens, r.cached_tokens, r.output_tokens), (100, 9000, 50));
        assert!(r.cost_usd > 0.0 && r.cost_usd < 0.01);
        assert!(parse_reply(&json!({ "error": { "message": "invalid x-api-key" } })).unwrap_err().contains("invalid"));
    }
}
