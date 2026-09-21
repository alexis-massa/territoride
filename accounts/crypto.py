from cryptography.fernet import Fernet
from django.conf import settings


def _fernet() -> Fernet:
    """Build a Fernet cipher from the configured encryption key."""
    return Fernet(settings.TOKEN_ENCRYPTION_KEY)


def encrypt_token(value: str) -> str:
    """Encrypt a plaintext token for storage, or "" if value is empty."""
    return _fernet().encrypt(value.encode()).decode() if value else ""


def decrypt_token(value: str) -> str:
    """Decrypt a stored token, or "" if value is empty."""
    return _fernet().decrypt(value.encode()).decode() if value else ""
