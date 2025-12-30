<?php

namespace App\Models;

use Bt\Solar\SystemEvent;
use App\Casts\ProtoAsJson;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Factories\HasFactory;

class Event extends Model
{
    use HasFactory;

    public function __toString() {
        return parent::__toString();
    }

    protected $casts = [
        'event' => ProtoAsJson::class.':'.SystemEvent::class,
        'timestamp' => 'immutable_datetime',
    ];
    
}
