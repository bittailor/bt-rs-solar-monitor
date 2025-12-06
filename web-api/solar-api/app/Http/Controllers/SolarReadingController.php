<?php

namespace App\Http\Controllers;

use Bt\Solar\Upload;
use DateTimeImmutable;
use App\Models\SolarReading;
use Illuminate\Http\Request;

class SolarReadingController extends Controller
{
    public function upload(Request $request)
    {
        $content = $request->getContent();
        $upload = new Upload();
        $upload->mergeFromString($content);
        $n = $upload->getEntries()->count();
        $startTimestamp = $upload->getStartTimestamp();
        foreach ($upload->getEntries() as $entry) {
            $reading = $entry->getReading();
            $timestamp = (new DateTimeImmutable())->setTimestamp($startTimestamp + $entry->getOffsetInSeconds());
            $solarReading = new SolarReading;
            $factor = 1000.0;
            $solarReading->battery_voltage = $reading->getBatteryVoltage() / $factor;
            $solarReading->battery_current = $reading->getBatteryCurrent() / $factor;
            $solarReading->panel_voltage = $reading->getPanelVoltage() / $factor;
            $solarReading->panel_power = $reading->getPanelPower();
            $solarReading->load_current = $reading->getLoadCurrent()  / $factor;
            $solarReading->recorded_at = $timestamp;
            $solarReading->save();
        }
        return response('', 200)->withHeaders([]);
    }
}
