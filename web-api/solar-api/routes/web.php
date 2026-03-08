<?php

use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Route;
use App\Http\Controllers\DashboardController;

Route::get('/', function () {
    return view('welcome');
});

Route::get('/info', function () {
    $info = array();
    $info['session'] = DB::select('SELECT @@SESSION.time_zone;');
    $info['system'] = DB::select('SELECT @@system_time_zone;');
    return $info; 
});


Route::get('/events', [DashboardController::class, 'events']);
Route::get('/readings', [DashboardController::class, 'readings']);
