use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

const TOKEN_TTL_SECS: i64 = 24 * 60 * 60; // 24 hours

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
}

pub fn create_token(username: &str, secret: &[u8]) -> String {
    let exp = chrono::Utc::now().timestamp() + TOKEN_TTL_SECS;
    let claims = Claims {
        sub: username.to_string(),
        exp,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret))
        .expect("JWT encoding should not fail")
}

pub fn validate_token(token: &str, secret: &[u8]) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &Validation::default())?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-key-for-jwt";

    #[test]
    fn create_and_validate_token() {
        let token = create_token("admin", SECRET);
        let claims = validate_token(&token, SECRET).unwrap();
        assert_eq!(claims.sub, "admin");
    }

    #[test]
    fn invalid_token() {
        let result = validate_token("invalid.token.here", SECRET);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_secret() {
        let token = create_token("admin", SECRET);
        let result = validate_token(&token, b"wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn expired_token() {
        let exp = chrono::Utc::now().timestamp() - 100; // already expired
        let claims = Claims {
            sub: "admin".to_string(),
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET),
        )
        .unwrap();
        let result = validate_token(&token, SECRET);
        assert!(result.is_err());
    }
}
