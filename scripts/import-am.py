import re
import os.path
from os import path
import json
import os
import csv

with open('AM.dat', newline='') as csvfile:
    reader = csv.reader(csvfile, delimiter='|', quotechar='"')
    for row in reader:
        try:
            callsign = row[4]
            result = re.search('^[^\d]+\d{1}', callsign).group()
            prefix_dir = "out/{prefix}".format(prefix=result)
            filename = "out/{prefix}/{callsign}.json".format(prefix=result,callsign=callsign)
        except:
            print("Unable to parse callsign")
            continue

        if not os.path.exists(filename):
            continue

        with open(filename, "r+") as f:
            data = json.load(f)
            data.update({ "class": row[5] })
            f.seek(0)
            f.truncate()
            json.dump(data, f)