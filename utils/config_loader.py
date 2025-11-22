import yaml

from utils.logging_config import logger


class ConfigLoader:
    def __init__(self, config_path: str):
        with open(config_path) as file:
            logger.info(
                f"Loading config from {config_path}",
            )
            self.yaml = yaml.safe_load(file)


config = ConfigLoader("config.yaml")