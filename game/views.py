from django.http import HttpRequest, HttpResponse
from django.shortcuts import render


def map_view(request: HttpRequest) -> HttpResponse:
    return render(request, "game/map.html")
