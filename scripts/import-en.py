import re
import os.path
from os import path
import json
import os
import csv

with open('EN.dat', newline='') as csvfile:
    reader = csv.reader(csvfile, delimiter='|', quotechar='"')
    for row in reader:
        callsign = row[4]
        try:
            result = re.search('^[^\d]+\d{1}', callsign).group()
            prefix_dir = "out/{prefix}".format(prefix=result)
            filename = "out/{prefix}/{callsign}.json".format(prefix=result,callsign=callsign)
        except:
            print("Unable to parse callsign {callsign}".format(callsign=callsign))
            continue

        if not os.path.exists(prefix_dir):
            os.makedirs(prefix_dir)

        try:
            json_data = {"call": callsign, "op": row[7], "address": row[15], "qth": row[16], "state": row[17], "zip": row[18]}
        except:
            print("Unable to process record for {callsign}: {row}".format(callsign=callsign,row=row))

        if not os.path.exists(filename):
            file1 = open(filename, "w")
            file1.write(json.dumps(json_data))
            file1.close()
        else:
            with open(filename, "r+") as f:
                data = json.load(f)
                data.update(json_data)
                f.seek(0)
                f.truncate()
                json.dump(data, f)
        