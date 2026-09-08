from typing import Any

import requests
from django.conf import settings
from django.core.management.base import BaseCommand, CommandError, CommandParser

PUSH_SUBSCRIPTIONS_URL = "https://www.strava.com/api/v3/push_subscriptions"


class Command(BaseCommand):
    help = "Create, view, or delete this app's Strava webhook subscription (Strava allows only one)"

    def add_arguments(self, parser: CommandParser) -> None:
        """Declare the command's arguments."""
        parser.add_argument("action", choices=["create", "view", "delete"])
        parser.add_argument("--callback-url", help="Public HTTPS URL for 'create', e.g. https://x/accounts/strava/webhook/")

    def handle(self, *args: Any, **options: Any) -> None:
        """Dispatch to the requested push-subscription action.

        Args:
            *args: Unused positional arguments from Django's command framework.
            **options: Parsed options; "action" and "callback_url" are used.
        """
        if options["action"] == "create":
            self._create(options["callback_url"])
        elif options["action"] == "view":
            self._view()
        else:
            self._delete()

    def _check(self, response: requests.Response) -> None:
        """Raise with Strava's actual error body, not just the generic status message.

        Args:
            response: The API response to check.
        """
        if not response.ok:
            raise CommandError(f"{response.status_code}: {response.text}")

    def _create(self, callback_url: str | None) -> None:
        """Register this app's webhook callback URL with Strava."""
        if not callback_url:
            raise CommandError("--callback-url is required to create a subscription")
        response = requests.post(
            PUSH_SUBSCRIPTIONS_URL,
            data={
                "client_id": settings.STRAVA_CLIENT_ID,
                "client_secret": settings.STRAVA_CLIENT_SECRET,
                "callback_url": callback_url,
                "verify_token": settings.STRAVA_WEBHOOK_VERIFY_TOKEN,
            },
            timeout=15,
        )
        self._check(response)
        self.stdout.write(f"Created: {response.json()}")

    def _view(self) -> None:
        """List this app's current webhook subscription(s), if any."""
        response = requests.get(
            PUSH_SUBSCRIPTIONS_URL,
            params={
                "client_id": settings.STRAVA_CLIENT_ID,
                "client_secret": settings.STRAVA_CLIENT_SECRET,
            },
            timeout=15,
        )
        self._check(response)
        self.stdout.write(str(response.json()))

    def _delete(self) -> None:
        """Delete every existing webhook subscription for this app."""
        response = requests.get(
            PUSH_SUBSCRIPTIONS_URL,
            params={
                "client_id": settings.STRAVA_CLIENT_ID,
                "client_secret": settings.STRAVA_CLIENT_SECRET,
            },
            timeout=15,
        )
        self._check(response)
        for subscription in response.json():
            delete_response = requests.delete(
                f"{PUSH_SUBSCRIPTIONS_URL}/{subscription['id']}",
                params={
                    "client_id": settings.STRAVA_CLIENT_ID,
                    "client_secret": settings.STRAVA_CLIENT_SECRET,
                },
                timeout=15,
            )
            self._check(delete_response)
            self.stdout.write(f"Deleted subscription {subscription['id']}")
