import time
from collections import deque
from be import listen
import threading

def logger():
    while True:
        print(f"Current global queue: {global_queue}")
        time.sleep(10)

if __name__ == '__main__':
    global_queue = deque()

    listener_thread = threading.Thread(target=listen, args=(global_queue,), daemon=True)
    listener_thread.start()

    logger_thread = threading.Thread(target=logger)
    logger_thread.start()
    logger_thread.join()


