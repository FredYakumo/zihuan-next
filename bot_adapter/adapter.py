from utils.logging_config import logger
from utils.message_store import init_message_store, store_message
import websockets
import json
import bot_adapter.event as event
from bot_adapter.models.event_model import MessageEvent
from bot_adapter.models.message import convert_message_from_json
import redis
from utils.config_loader import config

class BotAdapter:
    def __init__(self, url: str, token: str):
        self.url = url
        self.token = token

        self.event_process_func = {
            "private": event.process_friend_message,
            "group": event.process_group_message
        }

        init_message_store(config, logger)

    def start(self):
        async def connect():
            async with websockets.connect(self.url, additional_headers={"Authorization": f"Bearer {self.token}"}) as websocket:
                logger.info("Connect to the qq bot server.")
                logger.info("Connect to the qq bot server succeeded.")
                while True:
                    message = await websocket.recv()
                    self.bot_event_process(message)

        import asyncio
        asyncio.run(connect())

    def bot_event_process(self, message: str | bytes):
        logger.debug(f"Received message: {message}")
        if not isinstance(message, str):
            logger.warning("Received non-string message, ignoring.")
            return

        try:
            message_json: dict = json.loads(message)
            if "message_type" not in message_json:
                logger.debug("Ignoring non-message event.")
                return

            message_id = message_json.get("message_id")
            store_message(message_id, message, logger=logger)

            event_model = MessageEvent(
                message_id=message_json["message_id"],
                message_type=message_json["message_type"],
                sender=message_json["sender"],
                message_list=[convert_message_from_json(message) for message in message_json.get("message", [])]
            )

            if event_model.message_type in self.event_process_func:
                self.event_process_func[event_model.message_type](event_model)
        except Exception as e:
            logger.error(f"Error processing event: {e}")