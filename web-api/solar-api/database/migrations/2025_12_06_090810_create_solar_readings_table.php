<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    /**
     * Run the migrations.
     */
    public function up(): void
    {
        Schema::create('solar_readings', function (Blueprint $table) {
            $table->id();

            $table->float('battery_voltage');
            $table->float('battery_current');
            $table->float('panel_voltage');
            $table->float('panel_power');
            $table->float('load_current');
            $table->timestamp('recorded_at');

            $table->timestamps();
        });
    }

    /**
     * Reverse the migrations.
     */
    public function down(): void
    {
        Schema::dropIfExists('solar_readings');
    }
};
