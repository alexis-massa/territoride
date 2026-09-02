from cryptography.fernet import Fernet
from django.conf import settings


def _fernet() -> Fernet:
    """Build a Fernet cipher from the configured encryption key.

    Returns:
        A ready-to-use Fernet instance.
    """
    return Fernet(settings.TOKEN_ENCRYPTION_KEY)


def encrypt_token(value: str) -> str:
    """Encrypt a plaintext token for storage.

    Args:
        value: The plaintext token.

    Returns:
        The Fernet-encrypted ciphertext, or "" if value is empty.
    """
    return _fernet().encrypt(value.encode()).decode() if value else ""


def decrypt_token(value: str) -> str:
    """Decrypt a stored token.

    Args:
        value: The Fernet-encrypted ciphertext.

    Returns:
        The decrypted plaintext, or "" if value is empty.
    """
    return _fernet().decrypt(value.encode()).decode() if value else ""
