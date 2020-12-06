var mapView = null;
var markers = [];
var lastTime = null;

var icons = {};
var bands = ["160m","80m","40m","30m","20m","17m","15m","12m","10m","6m","2m","1.25cm","70cm","33cm","unknown"];
bands.forEach(function (band) {
    // Default CQ/LoTW icon
    icons[band] = L.icon({
        iconUrl: 'markers/small-marker-'+band+'.png',
        iconSize: [12, 20],
        iconAnchor: [6, 20],
        shadowUrl: 'small-marker-shadow.png',
        shadowSize: [18,18],
        shadowAnchor: [6, 18]
    });
    // Default (CQ/No LoTW) icon
    icons[band + "empty"] = L.icon({
        iconUrl: 'markers/small-marker-'+band+'-empty.png',
        iconSize: [12, 20],
        iconAnchor: [6, 20],
        shadowUrl: 'small-marker-shadow.png',
        shadowSize: [18,18],
        shadowAnchor: [6, 18]
    });
    // Small icon
    icons[band + "small"] = L.icon({
        iconUrl: 'markers/small-marker-'+band+'-empty.png',
        iconSize: [6, 10],
        iconAnchor: [3, 10],
        shadowUrl: 'small-marker-shadow.png',
        shadowSize: [9,9],
        shadowAnchor: [3, 9]
    });
});

var workedStates = [];
var lotwConfirmed = [];
var geoJson = null;
var statesOverlay = null;
var statesHidden = true;

function stateStyle(feature) {
    var worked = false;
    var lotw = lotwConfirmed.includes(feature.properties.name);
    if (lotw) {
        worked = true;
    } else {
        worked = workedStates.includes(feature.properties.name);
    }

    return statesHidden ? {
        opacity: 0,
        fillOpacity: 0,
    } : {
        fillColor: lotw ? "#00d1b2" : (worked ? "#FFEDA0" : "#ebebeb"),
        weight: 2,
        opacity: worked ? 0.7 : 0.7,
        color: 'white',
        dashArray: '3',
        fillOpacity: worked ? 0.7 : 0.7
    };
}

function updateStateOverlay() {
    if (geoJson != null) {
        geoJson.setStyle(stateStyle);
    }
}

function addStatesOverlay() {
    if (statesOverlay != null && geoJson == null && mapView != null) {
        geoJson = L.geoJson(statesOverlay, { style: stateStyle });
        geoJson.addTo(mapView);
    }
}

function initMap() {
    mapView = L.map('map').setView([0.0, 0.0], 2);
    L.tileLayer('https://api.mapbox.com/styles/v1/{id}/tiles/{z}/{x}/{y}?access_token=pk.eyJ1Ijoia2s0d2pzIiwiYSI6ImNraTFnY28xNDAwZ3Ayd3BhcGs1aTF2MzUifQ.HKwoDr52uGnmnllpgESJIg', {
        attribution: 'Map data &copy; <a href=\"https://www.openstreetmap.org/\">OpenStreetMap</a> contributors, <a href=\"https://creativecommons.org/licenses/by-sa/2.0/\">CC-BY-SA</a>, Imagery © <a href=\"https://www.mapbox.com/\">Mapbox</a>',
        maxZoom: 18,
        id: 'mapbox/light-v10',
        tileSize: 512,
        zoomOffset: -1,
        accessToken: 'your.mapbox.access.token'
    }).addTo(mapView);
    addStatesOverlay();
}

var currentPopup = null;

// add spot to map
function addMarker(call, lat, lon, spotOn, freq, bandName, lotw, cq, mode) {
    var bandIcon = bandName;
    if (lastTime == null || lastTime != spotOn) {
        // update previous spotOn period with new icons
        if (markers.length > 0) {
            markers[markers.length-1].forEach(function (marker) {

            });
        }

        // start a new time period first run or if new spotOn
        lastTime = spotOn;
        let tmp = [];
        markers.push(tmp);

        // remove spots > 10 time periods back
        if (markers.length > 10) {
            let tmp = markers.shift();
            tmp.forEach(function(entry) {
                entry.remove();
            });
        }
    }

    // lotw users get an icon with a dot
    if (cq && !lotw) {
        bandIcon = bandName + "empty";
    }

    // use a small icon if spot is not cq
    if (!cq) {
        bandIcon = bandName + "small";
    }

    // add spot marker to map
    let marker = L.marker([lat, lon], {icon: icons[bandIcon]}).addTo(mapView);
    marker.bindPopup("<p><b>"+call+"</b></p><p>" + bandName + " " + mode + "</p><p>LoTW: " + (lotw ? "Yes" : "No") + "</p");
    markers[markers.length-1].push(marker);
}

var frequency = null;
var receiverMode = null;
var filterHigh = null;
var filterLow = null;

var frequencyStart = null;
var frequencyStop = null;
var marker = null;
var waterfall = null;
var currentMode = null;

function initWaterfallNav(mode, freq, high, low) {
    frequency = freq;
    receiverMode = mode;
    filterHigh = high;
    filterLow = low;

    marker = document.getElementById("receiver-marker");
    waterfall = document.getElementById("waterfall");
    switch (mode) {
        case "FT8":
        case "WSPR":
        case "FT4":
        case "JT9":
            marker.style.display = "block";
            marker.style.borderRight = "0px";
            marker.style.borderLeft = "0px";
            marker.style.paddingLeft = "0px";
            marker.style.paddingRight = "0px";
        case "LSB":
        case "DigiL":
            marker.style.display = "block";
            marker.style.borderRight = "1px solid rgba(255, 255, 255, 0.7)";
            marker.style.borderLeft = "0px";
            marker.style.paddingLeft = "0px";
            break;
        case "USB":
        case "DigiU":
            marker.style.display = "block";
            marker.style.borderLeft = "1px solid rgba(255, 255, 255, 0.7)";
            marker.style.borderRight = "0px";
            marker.style.paddingRight = "0px";
            break;
        default:
            marker.style.display = "none";
    }
    updateWaterfallNav();
}

function updateWaterfallNav() {
    if (frequencyStart == null || frequencyStop == null || filterHigh == null || filterLow == null) {
        marker.style.display = "none";
        return;
    }
    marker.style.display = "block";

    let canvasWidth = waterfall.clientWidth;
    let hzPerPixel = (frequencyStop - frequencyStart) / canvasWidth;
    switch (receiverMode) {
        case "FT8":
        case "WSPR":
        case "FT4":
        case "JT9":
            var filterWidth = Math.ceil(Math.abs(filterHigh) / hzPerPixel);
            marker.style.width = filterWidth + "px";
            var offsetInHz = (frequency - frequencyStart) + filterLow;
            var offsetInPixels = Math.ceil(offsetInHz / hzPerPixel);
            marker.style.left = offsetInPixels + "px";
            break;
        case "LSB":
        case "DigiL":
            var lowInHz = Math.abs(filterHigh);
            var lowRealPixels = Math.round(lowInHz / hzPerPixel);
            var widthInHz = Math.abs(filterLow);
            var realWidth = Math.ceil(widthInHz / hzPerPixel) - lowRealPixels;
            var offsetInHz = frequency - frequencyStart;
            var offsetInPixels = Math.ceil(offsetInHz / hzPerPixel) - realWidth;
            marker.style.left = offsetInPixels + "px";
            marker.style.width = realWidth + "px";
            marker.style.paddingRight = lowRealPixels + "px";
            break;
        case "USB":
        case "DigiU":
            var offsetInHz = (frequency - frequencyStart) + filterLow;
            var offsetInPixels = Math.ceil(offsetInHz / hzPerPixel);
            marker.style.left = offsetInPixels + "px";
            var lowRealPixels = (Math.abs(filterLow) / hzPerPixel);
            marker.style.paddingLeft = lowRealPixels + "px";
            var realWidth = Math.ceil(Math.abs(filterHigh) / hzPerPixel) - lowRealPixels;
            marker.style.width = realWidth + "px";
            break;
    }
}