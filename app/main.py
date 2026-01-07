from collections import deque
import time
from be import listen_to_clipboard
import threading
from typing import Deque

def logger():
    while True:
        print(f"Current global queue: {global_queue}")
        time.sleep(10)

if __name__ == '__main__':
    global_queue: Deque[str] = deque()

    listener_thread = threading.Thread(target=listen_to_clipboard, args=(global_queue,), daemon=True)
    listener_thread.start()

    logger_thread = threading.Thread(target=logger)
    logger_thread.start()
    logger_thread.join()


