import re
import os.path
from os import path
import json
import os
import csv

with open('lotw-user-activity.csv', newline='') as csvfile:
    reader = csv.reader(csvfile, delimiter=',', quotechar='"')
    for row in reader:
        callsign = row[0]
        try:
            result = re.search('^[^\d]+\d{1}', callsign).group()
            prefix_dir = "out/{prefix}".format(prefix=result)
            filename = "out/{prefix}/{callsign}.json".format(prefix=result,callsign=callsign)
        except:
            print("Unable to parse callsign {callsign}".format(callsign=callsign))
            continue

        if not os.path.exists(filename):
            continue

        with open(filename, "r+") as f:
            data = json.load(f)
            data.update({"lotw": True, "last_lotw_upload": "{date}T{time}Z".format(date=row[1],time=row[2]) })
            f.seek(0)
            f.truncate()
            json.dump(data, f)