<?php

namespace App\Http\Controllers;

use App\Models\Event;
use Illuminate\Http\Request;

class DashboardController extends Controller
{
    public function events(Request $request) {
        $events = Event::orderBy('timestamp','desc')->limit(20)->get();
        $now = now();
        return view('events', ['events' => $events, 'now' => $now]);
    }
}
