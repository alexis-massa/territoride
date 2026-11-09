from django.db import migrations


def backfill_visits(apps, schema_editor):
    """Rebuild each player's latest visit per cell from the activities still on file."""
    import h3

    from game.capture import captured_cells
    from game.grid import RESOLUTION, cell_centroid

    Activity = apps.get_model("game", "Activity")
    CellVisit = apps.get_model("game", "CellVisit")

    latest = {}
    for activity in Activity.objects.exclude(track=None).order_by("recorded_at").iterator():
        points = [(lat, lng) for lng, lat in activity.track.coords]
        if len(set(points)) == 1:  # start-point-only activity, captures its one tile
            cells = {h3.latlng_to_cell(*points[0], RESOLUTION)}
        else:
            cells = captured_cells(points)
        for cell_id in cells:
            latest[(activity.user_id, cell_id)] = activity
    CellVisit.objects.bulk_create(
        [
            CellVisit(
                user_id=user_id,
                cell_id=cell_id,
                centroid=cell_centroid(cell_id),
                activity=activity,
                visited_at=activity.recorded_at,
            )
            for (user_id, cell_id), activity in latest.items()
        ],
        batch_size=1000,
        ignore_conflicts=True,
    )


class Migration(migrations.Migration):

    dependencies = [
        ("game", "0010_cellvisit"),
    ]

    operations = [
        migrations.RunPython(backfill_visits, migrations.RunPython.noop),
    ]
