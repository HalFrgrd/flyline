#!/usr/bin/env python3
import os
import shutil
import random
import string

DIR_PATH = "/tmp/flyline_large_dir"

def random_string(min_len=1, max_len=60):
    length = random.randint(min_len, max_len)
    chars = string.ascii_letters + string.digits + "_-"
    return "".join(random.choice(chars) for _ in range(length))

def main():
    if os.path.exists(DIR_PATH):
        print(f"Cleaning up existing directory {DIR_PATH}...")
        shutil.rmtree(DIR_PATH)
    
    os.makedirs(DIR_PATH, exist_ok=True)
    print(f"Creating 5000 entries in {DIR_PATH}...")

    # We will keep a list of created files/folders to point symlinks to
    created_items = []

    for i in range(5000):
        name = f"item_{i}_" + random_string(1, 50)
        path = os.path.join(DIR_PATH, name)
        
        # Ensure name uniqueness
        while os.path.exists(path):
            name = f"item_{i}_" + random_string(1, 50)
            path = os.path.join(DIR_PATH, name)

        choice = random.choice(["dir", "file", "symlink"])
        
        if choice == "dir":
            os.makedirs(path, exist_ok=True)
            created_items.append(name)
        elif choice == "file":
            with open(path, "w") as f:
                f.write("dummy content\n")
            created_items.append(name)
        else:  # symlink
            # Point to a random existing item if any, otherwise a dummy path
            if created_items:
                target = random.choice(created_items)
            else:
                target = "item_0_dummy"
            os.symlink(target, path)

    print(f"Successfully created 5000 entries in {DIR_PATH}.")

if __name__ == "__main__":
    main()
