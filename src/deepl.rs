use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub struct DeepLClient {
    client: Client,
    auth_key: String,
    base_url: String,
}

impl DeepLClient {
    pub fn new(auth_key: String, use_free_api: bool) -> Self {
        let base_url = if use_free_api {
            "https://api-free.deepl.com".to_string()
        } else {
            "https://api.deepl.com".to_string()
        };

        Self {
            client: Client::new(),
            auth_key,
            base_url,
        }
    }

    pub fn translate_texts(
        &self,
        texts: &[String],
        source_lang: Option<&str>,
        target_lang: &str,
        glossary_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        self.translate_texts_with_context(texts, source_lang, target_lang, glossary_id, None)
    }

    pub fn translate_texts_with_context(
        &self,
        texts: &[String],
        source_lang: Option<&str>,
        target_lang: &str,
        glossary_id: Option<&str>,
        context: Option<&str>,
    ) -> Result<Vec<String>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let normalized_texts: Vec<String> = texts
            .iter()
            .map(|text| normalize_translation_text(text))
            .collect();

        let mut output = vec![String::new(); normalized_texts.len()];
        let mut request_texts = Vec::new();
        let mut request_positions = Vec::new();

        for (index, text) in normalized_texts.into_iter().enumerate() {
            if text.is_empty() {
                output[index] = String::new();
            } else {
                request_positions.push(index);
                request_texts.push(text);
            }
        }

        if request_texts.is_empty() {
            return Ok(output);
        }

        let mut body = json!({
            "text": request_texts,
            "target_lang": target_lang.to_uppercase(),
        });

        if let Some(source_lang) = source_lang {
            if !source_lang.trim().is_empty() {
                body["source_lang"] = Value::String(source_lang.trim().to_uppercase());
            }
        }

        if let Some(glossary_id) = glossary_id {
            if body.get("source_lang").is_some() {
                body["glossary_id"] = Value::String(glossary_id.to_string());
            }
        }

        if let Some(context) = context {
            let normalized_context = normalize_translation_text(context);
            if !normalized_context.is_empty() {
                body["context"] = Value::String(normalized_context);
            }
        }

        let response = self.post_json("/v2/translate", &body)?;
        let parsed: TranslateResponse = serde_json::from_value(response)
            .map_err(|e| format!("Некорректный ответ DeepL translate: {}", e))?;

        if parsed.translations.len() != request_positions.len() {
            return Err("DeepL вернул неполный результат перевода".to_string());
        }

        for (translated, position) in parsed
            .translations
            .into_iter()
            .zip(request_positions.into_iter())
        {
            output[position] = translated.text;
        }

        Ok(output)
    }

    pub fn create_glossary(
        &self,
        name: &str,
        source_lang: &str,
        target_lang: &str,
        entries_tsv: &str,
    ) -> Result<String, String> {
        let body = json!({
            "name": name,
            "dictionaries": [
                {
                    "source_lang": source_lang.to_lowercase(),
                    "target_lang": target_lang.to_lowercase(),
                    "entries": entries_tsv,
                    "entries_format": "tsv"
                }
            ]
        });

        let response = self.post_json("/v3/glossaries", &body)?;
        let glossary_id = response
            .get("glossary_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "DeepL не вернул glossary_id".to_string())?;

        Ok(glossary_id.to_string())
    }

    pub fn replace_glossary_dictionary(
        &self,
        glossary_id: &str,
        source_lang: &str,
        target_lang: &str,
        entries_tsv: &str,
    ) -> Result<(), String> {
        let body = json!({
            "source_lang": source_lang.to_lowercase(),
            "target_lang": target_lang.to_lowercase(),
            "entries": entries_tsv,
            "entries_format": "tsv"
        });

        let path = format!("/v3/glossaries/{}/dictionaries", glossary_id);
        self.put_json(&path, &body)?;
        Ok(())
    }

    pub fn list_glossaries(&self) -> Result<Vec<GlossaryMeta>, String> {
        let response = self.get_json("/v3/glossaries")?;
        let parsed: GlossaryListResponse = serde_json::from_value(response)
            .map_err(|e| format!("Некорректный ответ DeepL glossaries: {}", e))?;
        Ok(parsed.glossaries)
    }

    pub fn load_glossary_entries(
        &self,
        glossary_id: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let path = format!(
            "/v3/glossaries/{}/entries?source_lang={}&target_lang={}",
            glossary_id,
            source_lang.to_lowercase(),
            target_lang.to_lowercase()
        );

        let response = self.get_json(&path)?;
        let parsed: GlossaryEntriesResponse = serde_json::from_value(response)
            .map_err(|e| format!("Некорректный ответ DeepL glossary entries: {}", e))?;

        let dictionary = parsed
            .dictionaries
            .first()
            .ok_or_else(|| "DeepL не вернул словарь для выбранной языковой пары".to_string())?;

        Ok(parse_tsv_entries(&dictionary.entries))
    }

    pub fn find_alternatives(&self, text: &str, target_lang: &str) -> Result<Vec<String>, String> {
        let mut candidates = BTreeSet::new();

        let styles = [None, Some("simple"), Some("casual"), Some("business")];
        let tones = [None, Some("friendly"), Some("confident")];

        for style in styles {
            for tone in tones {
                if let Ok(item) = self.rephrase_once(text, target_lang, style, tone) {
                    if !item.trim().is_empty() && item.trim() != text.trim() {
                        candidates.insert(item);
                    }
                }
            }
        }

        if candidates.is_empty() {
            let translated = self.translate_texts(&[text.to_string()], None, target_lang, None)?;
            for item in translated {
                if !item.trim().is_empty() && item.trim() != text.trim() {
                    candidates.insert(item);
                }
            }
        }

        Ok(candidates.into_iter().collect())
    }

    fn rephrase_once(
        &self,
        text: &str,
        target_lang: &str,
        writing_style: Option<&str>,
        tone: Option<&str>,
    ) -> Result<String, String> {
        let mut body = json!({
            "text": [text],
            "target_lang": target_lang.to_lowercase(),
        });

        if let Some(style) = writing_style {
            body["writing_style"] = Value::String(style.to_string());
        }
        if let Some(tone) = tone {
            body["tone"] = Value::String(tone.to_string());
        }

        let response = self.post_json("/v2/write/rephrase", &body)?;
        let improved = response
            .get("improvements")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .ok_or_else(|| "DeepL не вернул варианты rephrase".to_string())?;

        Ok(improved.to_string())
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.auth_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .map_err(|e| format!("Ошибка запроса DeepL: {}", e))?;

        let status = response.status();
        let payload = response
            .text()
            .map_err(|e| format!("Ошибка чтения ответа DeepL: {}", e))?;

        if !status.is_success() {
            return Err(self.format_api_error(status.as_u16(), &payload));
        }

        serde_json::from_str(&payload)
            .map_err(|e| format!("Некорректный JSON DeepL: {}; payload={}", e, payload))
    }

    fn put_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .put(url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.auth_key))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .map_err(|e| format!("Ошибка запроса DeepL: {}", e))?;

        let status = response.status();
        let payload = response
            .text()
            .map_err(|e| format!("Ошибка чтения ответа DeepL: {}", e))?;

        if !status.is_success() {
            return Err(self.format_api_error(status.as_u16(), &payload));
        }

        serde_json::from_str(&payload)
            .map_err(|e| format!("Некорректный JSON DeepL: {}; payload={}", e, payload))
    }

    fn get_json(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.auth_key))
            .send()
            .map_err(|e| format!("Ошибка запроса DeepL: {}", e))?;

        let status = response.status();
        let payload = response
            .text()
            .map_err(|e| format!("Ошибка чтения ответа DeepL: {}", e))?;

        if !status.is_success() {
            return Err(self.format_api_error(status.as_u16(), &payload));
        }

        serde_json::from_str(&payload)
            .map_err(|e| format!("Некорректный JSON DeepL: {}; payload={}", e, payload))
    }

    fn format_api_error(&self, status_code: u16, payload: &str) -> String {
        let message = extract_error_message(payload).unwrap_or_else(|| payload.to_string());

        if status_code == 456 {
            let usage_hint = self
                .fetch_usage_hint()
                .map(|hint| format!(" {}", hint))
                .unwrap_or_default();
            return format!(
                "DeepL 456: квота исчерпана. Обычно это лимит символов/документов текущего периода. {}{}",
                message, usage_hint
            );
        }

        format!("DeepL {}: {}", status_code, message)
    }

    fn fetch_usage_hint(&self) -> Option<String> {
        let url = format!("{}{}", self.base_url, "/v2/usage");
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.auth_key))
            .send()
            .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let payload = response.text().ok()?;
        let usage: UsageResponse = serde_json::from_str(&payload).ok()?;

        let mut parts = Vec::new();

        if let (Some(count), Some(limit)) = (usage.character_count, usage.character_limit) {
            parts.push(format!("символы: {}/{}", count, limit));
        }

        if let (Some(count), Some(limit)) = (usage.document_count, usage.document_limit) {
            parts.push(format!("документы: {}/{}", count, limit));
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!("Текущее usage: {}.", parts.join(", ")))
        }
    }
}

fn normalize_translation_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_error_message(payload: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(payload).ok()?;
    parsed
        .get("message")
        .and_then(Value::as_str)
        .map(|value| value.to_string())
}

fn parse_tsv_entries(entries: &str) -> Vec<(String, String)> {
    entries
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }

            let mut parts = trimmed.splitn(2, '\t');
            let source = parts.next()?.trim();
            let target = parts.next()?.trim();
            if source.is_empty() || target.is_empty() {
                return None;
            }

            Some((source.to_string(), target.to_string()))
        })
        .collect()
}

#[derive(Deserialize)]
struct TranslateResponse {
    translations: Vec<TranslateItem>,
}

#[derive(Deserialize)]
struct TranslateItem {
    text: String,
}

#[derive(Deserialize)]
pub struct GlossaryMeta {
    pub glossary_id: String,
    pub name: String,
    pub dictionaries: Vec<GlossaryDictionaryMeta>,
}

#[derive(Deserialize)]
pub struct GlossaryDictionaryMeta {
    pub source_lang: String,
    pub target_lang: String,
}

#[derive(Deserialize)]
struct GlossaryListResponse {
    glossaries: Vec<GlossaryMeta>,
}

#[derive(Deserialize)]
struct GlossaryEntriesResponse {
    dictionaries: Vec<GlossaryEntriesDictionary>,
}

#[derive(Deserialize)]
struct GlossaryEntriesDictionary {
    entries: String,
}

#[derive(Deserialize)]
struct UsageResponse {
    character_count: Option<u64>,
    character_limit: Option<u64>,
    document_count: Option<u64>,
    document_limit: Option<u64>,
}
