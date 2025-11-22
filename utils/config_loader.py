import os
import yaml

from utils.logging_config import logger


class ConfigLoader:
    def __init__(self, config_path: str):
        if not os.path.exists(config_path):
            logger.warning(
                f"Config file {config_path} does not exist. Using empty config."
            )
            self.yaml = {}
            return
        with open(config_path) as file:
            logger.info(
                f"Loading config from {config_path}",
            )
            self.yaml = yaml.safe_load(file)


config = ConfigLoader("config.yaml")