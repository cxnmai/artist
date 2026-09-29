-- Minimal buffered Chat Completions adapter for dev/mock-model/server.py.
artist.provider({
  name = "local-mock",
  models = {"artist-mock"},
  adapter = "chat_completions",
  endpoint = "http://127.0.0.1:8765/v1/chat/completions",
})
