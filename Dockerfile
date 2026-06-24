FROM python:3.13-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY clean_junk.py scheduler.py ./
RUN useradd --create-home --uid 10001 cleaner \
    && mkdir -p /data \
    && chown cleaner:cleaner /data

USER cleaner

VOLUME ["/data"]
CMD ["python", "/app/scheduler.py"]
