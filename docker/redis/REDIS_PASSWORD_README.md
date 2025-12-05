# Redis Password Configuration

The Redis service in this project is secured with a password. **The password is set in the `redis.conf` configuration file, not via environment variables or `docker-compose.yaml`.**

## How to Set the Redis Password

1. Open (or create) the file at `docker/redis/redis.conf`.
2. Add the following line (replace with your own strong password):

    requirepass yourStrongPasswordHere

3. The `docker-compose.yaml` mounts this file into the container:

    volumes:
      - ./data:/data
      - ./redis.conf:/usr/local/etc/redis/redis.conf

4. After changing the password, restart the Redis container:

    ```sh
    docker-compose down
    docker-compose up -d
    ```

**Note:** If you previously had a `redis.conf` directory, delete it and replace it with a file named `redis.conf` containing your configuration.
