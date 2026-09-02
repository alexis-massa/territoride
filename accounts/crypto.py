from cryptography.fernet import Fernet
from django.conf import settings


def _fernet() -> Fernet:
    """Build a Fernet cipher from the configured encryption key.

    Returns:
        A ready-to-use Fernet instance.
    """
    return Fernet(settings.TOKEN_ENCRYPTION_KEY)


class EncryptedTextField:
    """Descriptor that transparently encrypts/decrypts a backing TextField."""

    def __init__(self, backing_field: str) -> None:
        """Bind this descriptor to the model field storing the ciphertext.

        Args:
            backing_field: Name of the TextField holding the encrypted value.
        """
        self.backing_field = backing_field

    def __get__(self, instance: object, owner: type) -> str:
        """Decrypt and return the stored value.

        Args:
            instance: The model instance being read, or None if accessed on the class.
            owner: The owning class.

        Returns:
            The decrypted plaintext, or "" if unset or accessed on the class.
        """
        if instance is None:
            return ""
        raw: str = getattr(instance, self.backing_field)
        return _fernet().decrypt(raw.encode()).decode() if raw else ""

    def __set__(self, instance: object, value: str) -> None:
        """Encrypt value and store it on the backing field.

        Args:
            instance: The model instance being written to.
            value: The plaintext value to encrypt and store.
        """
        encrypted = _fernet().encrypt(value.encode()).decode() if value else ""
        setattr(instance, self.backing_field, encrypted)
