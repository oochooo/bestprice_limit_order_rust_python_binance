# Generated by Django 5.0.8 on 2024-09-24 09:23

import django.contrib.postgres.fields
from django.db import migrations, models


class Migration(migrations.Migration):

    initial = True

    dependencies = []

    operations = [
        migrations.CreateModel(
            name="Order",
            fields=[
                (
                    "id",
                    models.BigAutoField(
                        auto_created=True,
                        primary_key=True,
                        serialize=False,
                        verbose_name="ID",
                    ),
                ),
                ("created_at", models.DateTimeField(auto_now_add=True)),
                (
                    "completed_at",
                    models.DateTimeField(blank=True, null=True),
                ),
                ("exchange", models.CharField(max_length=10)),
                (
                    "_type",
                    models.CharField(
                        choices=[("m", "market"), ("l", "limit")],
                        max_length=1,
                    ),
                ),
                (
                    "sz_type",
                    models.CharField(
                        choices=[("n", "notional"), ("q", "qty")],
                        max_length=1,
                    ),
                ),
                ("symbol", models.CharField(max_length=100)),
                ("price_expected", models.FloatField()),
                (
                    "price_actual",
                    models.FloatField(blank=True, null=True),
                ),
                (
                    "matched_qty",
                    models.FloatField(blank=True, null=True),
                ),
                ("info", models.JSONField(blank=True, null=True)),
                ("paper", models.BooleanField()),
                ("dry_run", models.BooleanField()),
                ("sz", models.FloatField()),
                ("executed", models.BooleanField(default=False)),
                ("reduce_only", models.BooleanField(default=False)),
                ("error", models.TextField(blank=True, null=True)),
                ("strategy", models.CharField(max_length=100)),
                ("liquidated", models.BooleanField(default=False)),
                (
                    "mids",
                    django.contrib.postgres.fields.ArrayField(
                        base_field=django.contrib.postgres.fields.ArrayField(
                            base_field=models.FloatField(), size=2
                        ),
                        null=True,
                        size=None,
                    ),
                ),
                (
                    "trades",
                    django.contrib.postgres.fields.ArrayField(
                        base_field=django.contrib.postgres.fields.ArrayField(
                            base_field=models.FloatField(), size=3
                        ),
                        null=True,
                        size=None,
                    ),
                ),
            ],
        ),
    ]
