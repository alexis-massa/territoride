from django.http import HttpRequest, HttpResponse
from django.shortcuts import render


def map_view(request: HttpRequest) -> HttpResponse:
    """Render the main world map page.

    Args:
        request: The incoming request.

    Returns:
        The rendered map page.
    """
    return render(request, "game/map.html")
