from cryptography.fernet import Fernet
from django.conf import settings


def _fernet() -> Fernet:
    return Fernet(settings.TOKEN_ENCRYPTION_KEY)


class EncryptedTextField:
    """Descriptor: transparently encrypts/decrypts the named backing TextField."""

    def __init__(self, backing_field: str) -> None:
        self.backing_field = backing_field

    def __get__(self, instance: object, owner: type) -> str:
        if instance is None:
            return ""
        raw: str = getattr(instance, self.backing_field)
        return _fernet().decrypt(raw.encode()).decode() if raw else ""

    def __set__(self, instance: object, value: str) -> None:
        encrypted = _fernet().encrypt(value.encode()).decode() if value else ""
        setattr(instance, self.backing_field, encrypted)
